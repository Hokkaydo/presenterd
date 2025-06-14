# Presenterd
This daemon is intended to be used with the [app](https://github.com/Hokkaydo/presenter-app)
Once up, it allows receiving queries through Bluetooth LE (Low-Energy).

The intended use case is to allow switching the slides during a presentation by discreetly pressing a button (volume keys) on the device.

Currently, the only tested and supported implementation is the Linux one. 
Although the code for Windows is already in the repository, it still has to be tested.

## Build instructions
- Have [Cargo](https://github.com/rust-lang/cargo) installed
- Clone the repository using `git clone https://github.com/Hokkaydo/presenterd`
- Enter the clone directory and run `cargo install`.
- The daemon is installed in your local `cargo` directory (usually `~/.cargo/bin/`). To launch it, execute `~/.cargo/bin/presenterd`
The daemon is now up and ready to receive BLE queries from the related application.
