use embassy_executor::Spawner;
use embassy_stm32::{
    Config, Peripherals,
    peripherals::{ADC1, DMA2_CH0, PA2},
    time::Hertz,
};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
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

//#[embassy_executor::main]
async fn new_main(spawner: Spawner) {
    // init
    let p = get_peripherals();

    spawner.spawn(adc_task(p.ADC1, p.PA2, p.DMA2_CH0)).unwrap();
    spawner.spawn(from_uart()).unwrap();
    spawner.spawn(to_rail()).unwrap();
    spawner.spawn(to_uart()).unwrap();

    // main loop
    loop {
        let command = TO_MAIN_LOOP.receive().await;
        match command {
            CommObject::Text(_) => TO_RAIL.send(command).await,
            CommObject::Err(_) => TO_UART.send(command).await,
        }
    }
}

#[embassy_executor::task]
async fn adc_task(adc1: ADC1, mut pa2: PA2, dma_ch: DMA2_CH0) {
    // sample adc and send to TO_MAIN_LOOP
}

#[embassy_executor::task]
async fn from_uart() {
    // read from usb/uart and write to TO_MAIN_LOOP
}

#[embassy_executor::task]
async fn to_rail() {
    // read from TO_RAIL and write to spi/rails
}

#[embassy_executor::task]
async fn to_uart() {
    // read from TO_UART and write to usb/uart
}
