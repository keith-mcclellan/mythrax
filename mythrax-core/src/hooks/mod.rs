pub mod adapters;
pub mod precompact;
pub mod shell;
pub mod stop;

use crate::contracts::HookResult;

pub fn emit_hook_result(res: anyhow::Result<HookResult>) {
    match res {
        Ok(hook_res) => {
            let json_str = serde_json::to_string(&hook_res).unwrap_or_default();
            println!("{}", json_str);
            if hook_res.exit_code != 0 {
                std::process::exit(hook_res.exit_code);
            }
        }
        Err(e) => {
            eprintln!("Mythrax hook error (non-blocking): {:?}", e);
            let fallback = HookResult {
                continue_: true,
                suppress_output: false,
                exit_code: 0,
                injected: None,
            };
            let json_str = serde_json::to_string(&fallback).unwrap_or_default();
            println!("{}", json_str);
        }
    }
}
