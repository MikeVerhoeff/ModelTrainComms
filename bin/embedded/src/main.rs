#![no_std]
#![no_main]

use cortex_m::singleton;
use embassy_executor::Spawner;
use embassy_stm32::{
    Config, Peripherals,
    adc::{Adc, RingBufferedAdc, SampleTime, Sequence},
    bind_interrupts,
    peripherals::{self, ADC1, DMA2_CH0, DMA2_CH3, PA2, PB3, PB5, SPI1, USB_OTG_FS},
    spi::{self, Spi},
    time::Hertz,
    usb::{self, Driver},
};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::Instant;
use embassy_usb::{
    Builder,
    class::cdc_acm::{CdcAcmClass, Receiver, Sender, State},
};
use interfaces::CommObject;
use {defmt_rtt as _, panic_probe as _};

static TO_MAIN_LOOP: Channel<CriticalSectionRawMutex, CommObject, 2> = Channel::new();
static TO_UART: Channel<CriticalSectionRawMutex, CommObject, 2> = Channel::new();
static TO_RAIL: Channel<CriticalSectionRawMutex, CommObject, 2> = Channel::new();

fn get_peripherals() -> Peripherals {
    let mut config = Config::default();
    {
        use embassy_stm32::rcc::*;
        config.rcc.hse = Some(Hse {
            freq: Hertz(25_000_000),
            mode: HseMode::Oscillator,
        });
        config.rcc.pll_src = PllSource::HSE;
        config.rcc.pll = Some(Pll {
            prediv: PllPreDiv::DIV15,
            mul: PllMul::MUL173,
            divp: Some(PllPDiv::DIV4), // 25mhz / 15 * 173 / 4 = 72.0833333Mhz.
            divq: Some(PllQDiv::DIV6), // 25mhz / 15 * 173 / 6 = 48.0555556Mhz.
            divr: None,
        });
        config.rcc.ahb_pre = AHBPrescaler::DIV1;
        config.rcc.apb1_pre = APBPrescaler::DIV2;
        config.rcc.apb2_pre = APBPrescaler::DIV1;
        config.rcc.sys = Sysclk::PLL1_P;
        config.rcc.mux.clk48sel = mux::Clk48sel::PLL1_Q;
    }
    embassy_stm32::init(config)
}

bind_interrupts!(struct Irqs {
    OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;
});

#[embassy_executor::main]
async fn new_main(_spawner: Spawner) {
    // init
    let p = get_peripherals();

    // Create the driver, from the HAL.
    let mut ep_out_buffer = [0u8; 256];
    let mut config = embassy_stm32::usb::Config::default();

    // Do not enable vbus_detection. This is a safe default that works in all boards.
    // However, if your USB device is self-powered (can stay powered on if USB is unplugged), you need
    // to enable vbus_detection to comply with the USB spec. If you enable it, the board
    // has to support it or USB won't work at all. See docs on `vbus_detection` for details.
    config.vbus_detection = false;

    let driver = Driver::new_fs(
        p.USB_OTG_FS,
        Irqs,
        p.PA12,
        p.PA11,
        &mut ep_out_buffer,
        config,
    );

    // Create embassy-usb Config
    let mut config = embassy_usb::Config::new(0xc0de, 0xcafe);
    config.manufacturer = Some("Embassy");
    config.product = Some("USB-serial example");
    config.serial_number = Some("12345678");

    // Create embassy-usb DeviceBuilder using the driver and config.
    // It needs some buffers for building the descriptors.
    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut control_buf = [0; 64];

    let mut state = State::new();

    let mut builder = Builder::new(
        driver,
        config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut [], // no msos descriptors
        &mut control_buf,
    );

    // Create classes on the builder.
    let class = CdcAcmClass::new(&mut builder, &mut state, 64);

    let (usb_tx, usb_rx) = class.split();

    embassy_futures::join::join5(
        adc_task(p.ADC1, p.PA2, p.DMA2_CH0),
        from_uart(usb_rx),
        to_rail(p.SPI1, p.PB3, p.PB5, p.DMA2_CH3),
        to_uart(usb_tx),
        main_loop(),
    )
    .await;
}

static mut ADC_SAMPLE_STORE: &'static mut [u16; 512] = &mut [0; 512];

async fn main_loop() {
    loop {
        let command = TO_MAIN_LOOP.receive().await;
        match command {
            CommObject::Text(_) => TO_RAIL.send(command).await,
            CommObject::Err(_) => TO_UART.send(command).await,
        }
    }
}

async fn adc_task(adc1: ADC1, mut pa2: PA2, dma_ch: DMA2_CH0) {
    // sample adc and send to TO_MAIN_LOOP

    const ADC_BUF_SIZE: usize = 1024;
    let adc_data: &mut [u16; ADC_BUF_SIZE] =
        singleton!(ADCDAT : [u16; ADC_BUF_SIZE] = [0u16; ADC_BUF_SIZE]).unwrap();

    let adc = Adc::new(adc1);

    let mut adc: RingBufferedAdc<embassy_stm32::peripherals::ADC1> =
        adc.into_ring_buffered(dma_ch, adc_data);

    adc.set_sample_sequence(Sequence::One, &mut pa2, SampleTime::CYCLES112);

    // Note that overrun is a big consideration in this implementation. Whatever task is running the adc.read() calls absolutely must circle back around
    // to the adc.read() call before the DMA buffer is wrapped around > 1 time. At this point, the overrun is so significant that the context of
    // what channel is at what index is lost. The buffer must be cleared and reset. This *is* handled here, but allowing this to happen will cause
    // a reduction of performance as each time the buffer is reset, the adc & dma buffer must be restarted.

    // An interrupt executor with a higher priority than other tasks may be a good approach here, allowing this task to wake and read the buffer most
    // frequently.
    let mut tic = Instant::now();
    //let mut buffer1 = [0u16; 512]; -> ADC_SAMPLE_STORE
    let _ = adc.start();
    loop {
        unsafe {
            match adc.read(ADC_SAMPLE_STORE).await {
                Ok(_data) => {
                    let toc = Instant::now();
                    // process the data
                    tic = toc;
                }
                Err(e) => {
                    //ADC_SAMPLE_STORE = [0u16; 512];
                    let _ = adc.start();
                }
            }
        }
    }
}

async fn from_uart<'a>(mut receiver: Receiver<'a, Driver<'a, USB_OTG_FS>>) {
    // read from usb/uart and write to TO_MAIN_LOOP

    let mut data = [0u8; 64];
    loop {
        receiver.read_packet(&mut data).await;

        // parse_data

        //TO_MAIN_LOOP.send(message)
    }
}

async fn to_rail(peri: SPI1, sck: PB3, mosi: PB5, tx_dma: DMA2_CH3) {
    // read from TO_RAIL and write to spi/rails

    let mut spi_config = spi::Config::default();
    spi_config.frequency = Hertz(1_000_000);

    let mut spi = Spi::new_txonly(peri, sck, mosi, tx_dma, spi_config);

    loop {
        let to_send = TO_RAIL.receive().await;

        // data = serialize(to_send)
        let data = [0b11010110u8; 8];

        spi.write(&data);
    }
}

async fn to_uart<'a>(mut sender: Sender<'a, Driver<'a, USB_OTG_FS>>) {
    // read from TO_UART and write to usb/uart

    let mut data = [0u8; 64];
    loop {
        let message = TO_UART.receive().await;

        // serialize(message, data)

        sender.write_packet(&mut data);
    }
}
