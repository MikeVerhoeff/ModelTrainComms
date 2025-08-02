#![no_std]
#![no_main]

use defmt::{panic, *};
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::mode;
use embassy_stm32::spi::{self, Spi};
use embassy_stm32::time::Hertz;
use embassy_stm32::usb::{Driver, Instance};
use embassy_stm32::{bind_interrupts, peripherals, usb, Config};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::driver::EndpointError;
use embassy_usb::Builder;
use {defmt_rtt as _, panic_probe as _};

#[defmt::panic_handler]
fn panic() -> ! {
    core::panic!("panic via `defmt::panic!`")
}

//#[embassy_executor::task]
//async fn logger_task(driver: Driver<'static, peripherals::USB_OTG_FS>) {
//   embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
//}

bind_interrupts!(struct Irqs {
    OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
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
    let p = embassy_stm32::init(config);
    //let p = embassy_stm32::init(Default::default());
    
    let mut spi_config  = spi::Config::default();
    spi_config.frequency = Hertz(1_000_000);

    let mut spi = Spi::new_txonly(p.SPI1, p.PB3, p.PB5, p.DMA2_CH3, spi_config);


    //loop {
        let mut buf = [0x0Au8; 4];
        unwrap!(spi.blocking_transfer_in_place(&mut buf));
        info!("xfer {=[u8]:x}", buf);
    //}

    let mut onboard_led = Output::new(p.PC13, Level::High, Speed::Low);
    
    let mut pin0 = Output::new(p.PB9, Level::High, Speed::Low);
    let mut pin1 = Output::new(p.PB8, Level::High, Speed::Low);
    let mut pin2 = Output::new(p.PB7, Level::High, Speed::Low);
    let mut pin3 = Output::new(p.PB6, Level::High, Speed::Low);
    //let mut pin4 = Output::new(p.PB5, Level::High, Speed::Low);
    //let mut pin5 = Output::new(p.PB4, Level::High, Speed::Low);
    //let mut pin6 = Output::new(p.PB3, Level::High, Speed::Low);
    let mut pin7 = Output::new(p.PA13, Level::High, Speed::Low);
    let mut pin8 = Output::new(p.PA0, Level::High, Speed::Low);
    let mut pin9 = Output::new(p.PA1, Level::High, Speed::Low);
    let mut pin10 = Output::new(p.PA9, Level::High, Speed::Low);
    let mut pin11 = Output::new(p.PA8, Level::High, Speed::Low);
    let mut pin12 = Output::new(p.PB15, Level::High, Speed::Low);
    let mut pin13 = Output::new(p.PB14, Level::High, Speed::Low);
    let mut pin14 = Output::new(p.PB13, Level::High, Speed::Low);
    let mut pin15 = Output::new(p.PB12, Level::High, Speed::Low);

    onboard_led.set_low();
    pin0.set_low();
    pin1.set_low();
    pin2.set_low();
    pin3.set_low();
    //pin4.set_low();
    //pin5.set_low();
    //pin6.set_low();
    pin7.set_low();
    pin8.set_low();
    pin9.set_low();
    pin10.set_low();
    pin11.set_low();
    pin12.set_low();
    pin13.set_low();
    pin14.set_low();
    pin15.set_low();

    onboard_led.set_high();

    // Create the driver, from the HAL.
    let mut ep_out_buffer = [0u8; 256];
    let mut config = embassy_stm32::usb::Config::default();

    // Do not enable vbus_detection. This is a safe default that works in all boards.
    // However, if your USB device is self-powered (can stay powered on if USB is unplugged), you need
    // to enable vbus_detection to comply with the USB spec. If you enable it, the board
    // has to support it or USB won't work at all. See docs on `vbus_detection` for details.
    config.vbus_detection = false;

    let driver = Driver::new_fs(p.USB_OTG_FS, Irqs, p.PA12, p.PA11, &mut ep_out_buffer, config);

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
    let mut class = CdcAcmClass::new(&mut builder, &mut state, 64);

    // Build the builder.
    let mut usb = builder.build();

    // Run the USB device.
    let usb_fut = usb.run();

    pin0.set_high();

    // Do stuff with the class!
    let echo_fut = async {
        loop {
            pin1.toggle();
            class.wait_connection().await;
            info!("Connected");
            let _ = echo(&mut class, &mut pin3, &mut spi).await;
            info!("Disconnected");
            pin2.toggle();
        }
    };

    // Run everything concurrently.
    // If we had made everything `'static` above instead, we could do this using separate tasks instead.
    join(usb_fut, echo_fut).await;
}



struct Disconnected {}

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => panic!("Buffer overflow"),
            EndpointError::Disabled => Disconnected {},
        }
    }
}

async fn echo<'d, T: Instance + 'd>(class: &mut CdcAcmClass<'d, Driver<'d, T>>, pin: &mut Output<'_>, spi: &mut Spi<'_, mode::Async>) -> Result<(), Disconnected> {
    let mut buf = [0; 64];
    let before = b"recived: '";
    let after = b"'\r\n";
    loop {
        let n = class.read_packet(&mut buf).await?;
        let data = &mut buf[..n];
        info!("data: {:x}", data);
        pin.toggle();
        class.write_packet(before).await?;
        class.write_packet(data).await?;
        class.write_packet(after).await?;
        unwrap!(spi.write(data).await);
        buf.fill(0u8);
    }
}