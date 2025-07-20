cargo build -p pc
cargo build --target thumbv7em-none-eabihf -p embedded --release
copy .\target\thumbv7em-none-eabihf\release\embedded .\target\thumbv7em-none-eabihf\release\embedded.elf
arm-none-eabi-objcopy -O ihex .\target\thumbv7em-none-eabihf\release\embedded.elf .\target\thumbv7em-none-eabihf\release\embedded.hex
arm-none-eabi-objcopy -O binary  .\target\thumbv7em-none-eabihf\release\embedded.elf .\target\thumbv7em-none-eabihf\release\embedded.bin