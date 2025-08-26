use embassy_stm32::gpio::Output;
use embassy_stm32::usb::{Driver, Instance};
use embassy_time::Instant;
use embassy_usb::Builder;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::driver::EndpointError;
use futures_util::{FutureExt, TryFutureExt};

use crate::Disconnected;

#[embassy_executor::task]
async fn uart_task() {}

async fn echo<'d, T: Instance + 'd>(
    class: &mut CdcAcmClass<'d, Driver<'d, T>>,
) -> Result<(), Disconnected> {
    let mut buff = [0u8; 16];
    loop {
        let packet = class.read_packet(&mut buff);
        /* class.hasbytes {
            comm_object = decode.input(from_uart_channel)
            to_main_loop.send(comm_object)
        }
        from_system.hasMessage {
            class.send(from_system.encode)
        }
        */

        //let n = class.read_packet(&mut buf).await?;
        //let data = &mut buf[..n];
        //info!("data: {:x}", data);

        //class.write_packet(before).await?;
        //class.write_packet(data).await?;
        //class.write_packet(after).await?;
        //unwrap!(spi.write(data).await);
        //buf.fill(0u8);

        //class.write_packet(unsafe { core::mem::transmute::<&[u16; 512], &[u8; 1024]>(ADC_SAMPLE_STORE) }).await?;
    }
}
