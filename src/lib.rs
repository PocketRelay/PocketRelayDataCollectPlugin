#![allow(clippy::missing_safety_doc)]

use windows_sys::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};

use crate::servers::start_servers;

pub mod constants;
pub mod hooks;
pub mod logging;
pub mod pattern;
pub mod servers;

#[no_mangle]
#[allow(non_snake_case, unused_variables)]
unsafe extern "system" fn DllMain(dll_module: usize, call_reason: u32, _: *mut ()) -> bool {
    match call_reason {
        DLL_PROCESS_ATTACH => {
            use windows_sys::Win32::System::Console::AllocConsole;
            AllocConsole();

            logging::setup();
            servers::components::initialize();

            // Handles the DLL being attached to the game
            unsafe { hooks::hook() };

            // Spawn UI and prepare task set
            std::thread::spawn(|| {
                // Create tokio async runtime
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("Failed building the Runtime");

                runtime.block_on(async move {
                    start_servers();
                    // Block for CTRL+C to keep servers alive when window closes
                    _ = tokio::signal::ctrl_c().await;
                });
            });
        }
        DLL_PROCESS_DETACH => {
            use windows_sys::Win32::System::Console::FreeConsole;
            FreeConsole();
        }
        _ => {}
    }

    true
}
