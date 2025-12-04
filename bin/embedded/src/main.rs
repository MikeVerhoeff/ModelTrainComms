#![no_std]
#![no_main]

use core::cell::Cell;

use cortex_m::interrupt::Mutex;
use cortex_m::singleton;
use defmt::panic;
use embassy_executor::{Spawner, task};
use embassy_futures::yield_now;
use embassy_stm32::adc::{Adc, RingBufferedAdc, SampleTime, Sequence};
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::peripherals::{ADC1, DMA2_CH0, DMA2_CH3, PA2, PB3, PB5, SPI1, USB_OTG_FS};
use embassy_stm32::spi::Spi;
use embassy_stm32::time::Hertz;
use embassy_stm32::usb::{Driver, Instance};
use embassy_stm32::{Config, bind_interrupts, peripherals, usb};
use embassy_stm32::{Peripherals, spi};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_usb::Builder;
use embassy_usb::class::cdc_acm::{CdcAcmClass, Receiver, Sender, State};
use embassy_usb::driver::EndpointError;
use interfaces::{CommBytes, CommObject, MAX_PACKET_SIZE, encoding};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

static PANIC_LED: Mutex<Cell<Option<Output>>> = Mutex::new(Cell::new(None));

#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::interrupt::free(|cs| {
        // Try to retrieve the LED pin handle from the global resource.
        if let Some(mut led) = PANIC_LED.borrow(cs).take() {
            // --- LED ON LOGIC ---
            led.set_high(); // Turn on the LED to signal panic.
            for _ in 0..10_000 {
                cortex_m::asm::nop();
            }
            led.set_low();
            for _ in 0..10_000 {
                cortex_m::asm::nop();
            }
            led.set_high();

            // Put the pin back (politeness, not strictly required as we halt)
            PANIC_LED.borrow(cs).set(Some(led));
        }
    });

    loop {
        cortex_m::asm::wfi();
    }
}

bind_interrupts!(struct Irqs {
    OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;
});

static TO_MAIN_LOOP: Channel<CriticalSectionRawMutex, CommBytes, 2> = Channel::new();
static TO_UART: Channel<CriticalSectionRawMutex, CommBytes, 2> = Channel::new();
static TO_RAIL: Channel<CriticalSectionRawMutex, CommBytes, 2> = Channel::new();

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
static EP_OUT_BUFFER: StaticCell<[u8; 256]> = StaticCell::new();

static STATE: StaticCell<State<'static>> = StaticCell::new();

static CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static BOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();

async fn spi_message(text: &[u8]) {
    let mut message = [b' '; 256];
    //message[text.len()] = '\n';
    //message[255] = '\n';
    message.copy_from_slice(text);
    TO_RAIL.send(message).await;
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = get_peripherals();

    let mut onboard_led = Output::new(p.PC13, Level::High, Speed::Low);

    let mut pin0 = Output::new(p.PB9, Level::High, Speed::Low);
    let mut pin1 = Output::new(p.PB8, Level::High, Speed::Low);
    let mut pin2 = Output::new(p.PB7, Level::High, Speed::Low);
    let mut pin3 = Output::new(p.PB6, Level::High, Speed::Low);

    onboard_led.set_low();
    pin0.set_low();
    pin1.set_low();
    pin2.set_low();
    pin3.set_low();

    onboard_led.set_high();

    cortex_m::interrupt::free(|cs| {
        PANIC_LED.borrow(cs).set(Some(pin1));
    });

    // Create the driver, from the HAL.
    let mut config = embassy_stm32::usb::Config::default();

    // Do not enable vbus_detection. This is a safe default that works in all boards.
    // However, if your USB device is self-powered (can stay powered on if USB is unplugged), you need
    // to enable vbus_detection to comply with the USB spec. If you enable it, the board
    // has to support it or USB won't work at all. See docs on `vbus_detection` for details.
    config.vbus_detection = false;

    let config_descriptor = CONFIG_DESCRIPTOR.init([0; 256]);
    let bos_descriptor = BOS_DESCRIPTOR.init([0; 256]);
    let control_buf = CONTROL_BUF.init([0; 64]);
    let ep_out_buffer = EP_OUT_BUFFER.init([0; 256]);

    let state = STATE.init(State::new()); // Note the 'static lifetime of the returned State<&'static CS>

    let config = embassy_stm32::usb::Config::default();
    #[allow(static_mut_refs)]
    let driver = Driver::new_fs(
        p.USB_OTG_FS,
        Irqs,
        p.PA12,
        p.PA11,
        ep_out_buffer, // This is okay because `ep_out_buffer` is local to main
        config,
    );

    let usb_config = embassy_usb::Config::new(0xc0de, 0xcafe);

    let mut builder = Builder::new(
        driver,
        usb_config,
        config_descriptor,
        bos_descriptor,
        &mut [],
        control_buf,
    );

    let class = CdcAcmClass::new(&mut builder, state, 64);
    let (usb_tx, usb_rx) = class.split();

    // Build the builder.
    let mut usb = builder.build();

    pin0.set_high();

    //let _ = spawner.spawn(adc_task(p.ADC1, p.PA2, p.DMA2_CH0));
    let _ = spawner.spawn(to_rail(p.SPI1, p.PB3, p.PB5, p.DMA2_CH3));

    let _ = spawner.spawn(to_uart(usb_tx));
    //let _ = spawner.spawn(from_uart(usb_rx));

    let _ = spawner.spawn(main_loop());

    pin2.set_high();

    usb.run().await;
}

#[task]
async fn main_loop() {
    //spi_message(b"Start main loop").await;

    loop {
        yield_now().await;
        //let command_bytes = TO_MAIN_LOOP.receive().await;
        let mut command_bytes = [0u8; 256];
        command_bytes[0] = b'a';
        command_bytes[1] = b'b';
        command_bytes[2] = b'x';
        command_bytes[3] = b'y';
        //TO_RAIL.send(command_bytes).await;
        TO_UART.send(command_bytes).await;

        //spi_message(b"Main loop").await;

        /*let result: Result<CommObject, postcard::Error> = postcard::from_bytes(&command_bytes);
        if let Ok(command) = result {
            match command {
                CommObject::Text(_) => TO_RAIL.send(command_bytes).await,
                CommObject::Err(_) => TO_UART.send(command_bytes).await,
            }
        }*/
    }
}

#[task]
async fn adc_task(adc1: ADC1, mut pa2: PA2, dma_ch: DMA2_CH0) {
    //spi_message(b"Start adc_task").await;
    // sample adc and send to TO_MAIN_LOOP
    let mut adc_sample_store = [0; 512];

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
    //let mut tic = Instant::now();
    //let mut buffer1 = [0u16; 512]; -> ADC_SAMPLE_STORE
    let _ = adc.start();
    loop {
        match adc.read(&mut adc_sample_store).await {
            Ok(_data) => {
                //let toc = Instant::now();
                // process the data
                //tic = toc;
            }
            Err(_e) => {
                //ADC_SAMPLE_STORE = &mut [0u16; 512];
                adc_sample_store.fill(0u16);
                let _ = adc.start();
            }
        }
    }
}

#[task]
async fn from_uart(mut receiver: Receiver<'static, Driver<'static, USB_OTG_FS>>) {
    //spi_message(b"Start from_uart").await;

    // read from usb/uart and write to TO_MAIN_LOOP
    let mut data = [0u8; MAX_PACKET_SIZE];
    //let mut decoder = encoding::Encoder::new();
    loop {
        //yield_now().await;
        if let Ok(mut size) = receiver.read_packet(&mut data).await {
            //loop {
            /*yield_now().await;
            let (consumed, success) = decoder.encoded_input(&data, size);
            if success {
                let mut buffer: CommBytes = [0u8; MAX_PACKET_SIZE];
                let message_bytes = decoder.decode();
                buffer.clone_from_slice(message_bytes);
                TO_MAIN_LOOP.send(buffer).await;
            }
            size -= consumed;
            if consumed == 0 {
                break;
            }*/
            //}

            //let _ = TO_MAIN_LOOP.send(data);
        } else {
            //spi_message(b"uart receiver error").await;
        }

        // parse_data
    }
}

#[task]
async fn to_rail(peri: SPI1, sck: PB3, mosi: PB5, tx_dma: DMA2_CH3) {
    // read from TO_RAIL and write to spi/rails

    let mut spi_config = spi::Config::default();
    spi_config.frequency = Hertz(1_000_000);

    let mut spi = Spi::new_txonly(peri, sck, mosi, tx_dma, spi_config);

    spi.write(b"SPI Start\n").await;

    loop {
        let mut to_send = TO_RAIL.receive().await;
        to_send[32] = b'\n';
        //let to_send = [0x38u8, 0xB2u8, 0xC8u8, 0x3Fu8];
        let _ = spi.write(&to_send[0..33]).await;
    }
}

#[task]
async fn to_uart(mut sender: Sender<'static, Driver<'static, USB_OTG_FS>>) {
    //spi_message(b"Start to_uart").await;

    // read from TO_UART and write to usb/uart
    loop {
        let message = TO_UART.receive().await;

        // serialize(message, data)

        let mut debug_message = [b' '; 256];
        debug_message[0..7].copy_from_slice(b"Message");
        debug_message[8..12].copy_from_slice(&message[0..4]);

        //let _ = sender.write_packet("Test uart\n\r".as_bytes()).await;
        let result = sender.write_packet(&message[0..4]).await;

        debug_message[12..18].copy_from_slice(b"Result");

        if let Err(e) = result {
            match e {
                EndpointError::BufferOverflow => {
                    debug_message[18..32].copy_from_slice(b"BufferOverflow")
                }
                EndpointError::Disabled => debug_message[18..26].copy_from_slice(b"Disabled"),
            }
        } else {
            debug_message[18..20].copy_from_slice(b"Ok");
        }

        TO_RAIL.send(debug_message).await;
    }
}
