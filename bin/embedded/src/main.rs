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
use embassy_stm32::interrupt::InterruptExt;
use embassy_stm32::peripherals::{ADC1, DMA2_CH0, DMA2_CH3, PA2, PB3, PB5, PB6, SPI1, USB_OTG_FS};
use embassy_stm32::spi::Spi;
use embassy_stm32::time::Hertz;
use embassy_stm32::usb::{Driver, Instance};
use embassy_stm32::{Config, bind_interrupts, interrupt, peripherals, usb};
use embassy_stm32::{Peripherals, spi};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::Timer;
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
static TO_UART: Channel<CriticalSectionRawMutex, CommBytes, 4> = Channel::new();
static TO_RAIL: Channel<CriticalSectionRawMutex, CommBytes, 4> = Channel::new();

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
    message[..text.len()].copy_from_slice(text);
    TO_RAIL.send(message).await;
}
async fn usb_message(text: &[u8]) {
    let mut message_buffer = [b' '; 256];

    let message_object = CommObject::Text("USB_mesage_test\r\n".into());
    match postcard::to_slice(&message_object, &mut message_buffer) {
        Ok(_) => {
            spi_message(b"Usb message Serialized").await;
            TO_RAIL.send(message_buffer).await;
        }
        Err(_) => spi_message(b"Comm Object to_slice error").await,
    }
    TO_UART.send(message_buffer).await;

    /*let mut message = [b' '; 256];
    message[..text.len()].copy_from_slice(text);
    TO_UART.send(message).await;*/
}

static USB_RX: StaticCell<Receiver<'static, Driver<'static, USB_OTG_FS>>> = StaticCell::new();
static USB_TX: StaticCell<Sender<'static, Driver<'static, USB_OTG_FS>>> = StaticCell::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = get_peripherals();

    interrupt::DMA2_STREAM0.set_priority(interrupt::Priority::P0); // adc
    interrupt::OTG_FS.set_priority(interrupt::Priority::P1); // usb
    interrupt::OTG_FS_WKUP.set_priority(interrupt::Priority::P3);
    interrupt::DMA2_STREAM3.set_priority(interrupt::Priority::P2); //spi

    let mut onboard_led = Output::new(p.PC13, Level::High, Speed::Low);

    let mut pin0 = Output::new(p.PB9, Level::High, Speed::Low);
    let mut pin1 = Output::new(p.PB8, Level::High, Speed::Low);
    let mut pin2 = Output::new(p.PB7, Level::High, Speed::Low);

    onboard_led.set_low();
    pin0.set_low();
    pin1.set_low();
    pin2.set_low();

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
    let usb_tx = USB_TX.init(usb_tx);
    let usb_rx = USB_RX.init(usb_rx);

    // Build the builder.
    let mut usb = builder.build();

    pin0.set_high();

    let _ = spawner.spawn(adc_task(p.ADC1, p.PA2, p.DMA2_CH0, p.PB6));
    let _ = spawner.spawn(to_rail(p.SPI1, p.PB3, p.PB5, p.DMA2_CH3));

    let _ = spawner.spawn(to_uart(usb_tx));
    let _ = spawner.spawn(from_uart(usb_rx));

    let _ = spawner.spawn(main_loop());

    pin2.set_high();

    usb.run().await;
}

#[task]
async fn main_loop() {
    spi_message(b"Start main loop\n").await;

    loop {
        Timer::after_millis(100).await;
        spi_message(b"loop\n").await;
        //usb_message(b"USB\n").await;

        /*let result: Result<CommObject, postcard::Error> = postcard::from_bytes(&command_bytes);
        if let Ok(command) = result {
            match command {
                CommObject::Text(_) => TO_RAIL.send(command_bytes).await,
                CommObject::Err(_) => TO_UART.send(command_bytes).await,
            }
        }*/

        let message = TO_MAIN_LOOP.receive().await;
        TO_UART.send(message).await;
    }
}

#[task]
async fn adc_task(adc1: ADC1, mut pa2: PA2, dma_ch: DMA2_CH0, timing_debug_pin: PB6) {
    let mut timing_debug_pin = Output::new(timing_debug_pin, Level::High, Speed::Low);
    timing_debug_pin.set_low();

    spi_message(b"Start adc_task\n").await;
    // sample adc and send to TO_MAIN_LOOP
    let mut adc_sample_store = [0; 512];

    const ADC_BUF_SIZE: usize = 1024;
    let adc_data: &mut [u16; ADC_BUF_SIZE] =
        singleton!(ADCDAT : [u16; ADC_BUF_SIZE] = [0u16; ADC_BUF_SIZE]).unwrap();

    let adc = Adc::new(adc1);

    let mut adc: RingBufferedAdc<embassy_stm32::peripherals::ADC1> =
        adc.into_ring_buffered(dma_ch, adc_data);

    /* AI Values test/verify before use
    SampleTime Value, Sampling Cycles, Total Conversion Cycles (Tconv​=Cycles+12) ,Resulting Sample Rate (fADC​=21 MHz)
    CYCLES7,          7.5 cycles,      19.5 cycles,                              "1,076,923 SPS"
    CYCLES13,         13.5 cycles,     25.5 cycles,                              "823,529 SPS"
    CYCLES28,         28.5 cycles,     40.5 cycles,                              "518,518 SPS"
    CYCLES56,         56.5 cycles,     68.5 cycles,                              "306,569 SPS"
    CYCLES84,         84.5 cycles,     96.5 cycles,                              "217,616 SPS"
    CYCLES112,        112.5 cycles,    124.5 cycles,                             "168,674 SPS"
     */

    adc.set_sample_sequence(Sequence::One, &mut pa2, SampleTime::CYCLES56); // SampleTime::CYCLES112

    // Note that overrun is a big consideration in this implementation. Whatever task is running the adc.read() calls absolutely must circle back around
    // to the adc.read() call before the DMA buffer is wrapped around > 1 time. At this point, the overrun is so significant that the context of
    // what channel is at what index is lost. The buffer must be cleared and reset. This *is* handled here, but allowing this to happen will cause
    // a reduction of performance as each time the buffer is reset, the adc & dma buffer must be restarted.

    // An interrupt executor with a higher priority than other tasks may be a good approach here, allowing this task to wake and read the buffer most
    // frequently.
    //let mut tic = Instant::now();
    //let mut buffer1 = [0u16; 512]; -> ADC_SAMPLE_STORE

    let mut n = 0;

    let _ = adc.start();
    loop {
        match adc.read(&mut adc_sample_store).await {
            Ok(_data) => {
                //let toc = Instant::now();
                // process the data
                //tic = toc;
                timing_debug_pin.set_high();
                //spi_message(b"Sampling done\n").await;
                //for _ in 0..25_000 {
                //    cortex_m::asm::nop();
                //}
                n += 1;
                if n == 1000 {
                    n = 0;
                    let message = CommObject::Samples(
                        heapless::Vec::from_slice(&adc_sample_store[..64]).unwrap(),
                    );
                    let mut message_buffer = [0u8; 256];
                    let _ = postcard::to_slice(&message, &mut message_buffer);
                    TO_UART.send(message_buffer).await;

                    let message = CommObject::Samples(
                        heapless::Vec::from_slice(&adc_sample_store[64..64 * 2]).unwrap(),
                    );
                    let _ = postcard::to_slice(&message, &mut message_buffer);
                    TO_UART.send(message_buffer).await;

                    let message = CommObject::Samples(
                        heapless::Vec::from_slice(&adc_sample_store[64 * 2..64 * 3]).unwrap(),
                    );
                    let _ = postcard::to_slice(&message, &mut message_buffer);
                    TO_UART.send(message_buffer).await;

                    let message = CommObject::Samples(
                        heapless::Vec::from_slice(&adc_sample_store[64 * 3..64 * 4]).unwrap(),
                    );
                    let _ = postcard::to_slice(&message, &mut message_buffer);
                    TO_UART.send(message_buffer).await;
                }
                timing_debug_pin.set_low();
            }
            Err(_e) => {
                //ADC_SAMPLE_STORE = &mut [0u16; 512];
                spi_message(b"ADC Overrun!!\n").await;
                adc_sample_store.fill(0u16);
                let _ = adc.start();
            }
        }
    }
}

#[task]
async fn from_uart(receiver: &'static mut Receiver<'static, Driver<'static, USB_OTG_FS>>) {
    spi_message(b"Start from_uart\n").await;

    // read from usb/uart and write to TO_MAIN_LOOP
    let mut data = [0u8; MAX_PACKET_SIZE];
    let mut buffer_index = 0;

    loop {
        match receiver.read_packet(&mut data[buffer_index..]).await {
            Ok(size) if size <= MAX_PACKET_SIZE => {
                spi_message(b"USB received:\n").await;
                //spi_message(&data[buffer_index..buffer_index + size]).await;
                buffer_index += size;
                if buffer_index == MAX_PACKET_SIZE {
                    TO_MAIN_LOOP.send(data).await;
                    buffer_index = 0;
                }
            }
            Ok(_) => spi_message(b"Large packet\n").await,
            Err(EndpointError::BufferOverflow) => {
                spi_message(b"USB receive: buffer overflow\n").await
            }
            Err(EndpointError::Disabled) => {
                Timer::after_millis(100).await;
                spi_message(b"USB receive: disabled\n").await
            }
        }
    }
}

#[task]
async fn to_rail(peri: SPI1, sck: PB3, mosi: PB5, tx_dma: DMA2_CH3) {
    // read from TO_RAIL and write to spi/rails

    let mut spi_config = spi::Config::default();
    spi_config.frequency = Hertz(56_000);

    let mut spi = Spi::new_txonly(peri, sck, mosi, tx_dma, spi_config);

    loop {
        let to_send = TO_RAIL.receive().await;
        let _ = spi.write(&to_send[0..32]).await;
    }
}

#[task]
async fn to_uart(sender: &'static mut Sender<'static, Driver<'static, USB_OTG_FS>>) {
    spi_message(b"Start to_uart\n").await;

    // read from TO_UART and write to usb/uart
    loop {
        let message = TO_UART.receive().await;
        spi_message(b"USB Message out:\n").await;
        //TO_RAIL.send(message).await; // raw way to so spi_message

        if sender.dtr() {
            for i in 0..4 {
                let result = sender.write_packet(&message[64 * i..64 * (i + 1)]).await; // 64 byte packet size

                if let Err(e) = result {
                    match e {
                        EndpointError::BufferOverflow => {
                            spi_message(b"USB buffer Overflow\n").await
                        }
                        EndpointError::Disabled => {
                            spi_message(b"USB Disabled\n").await;
                            Timer::after_millis(100).await;
                        }
                    }
                } else {
                    spi_message(b"USB Ok\n").await;
                }
            }
            // flush
            let _ = sender.write_packet(&[]).await;
        } else {
            spi_message(b"USB not dtr\n").await;
        }
    }
}
