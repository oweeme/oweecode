// Entry point: declares every feature module and wires their commands into
// the Tauri app. Each module owns one area of functionality end to end
// (types, state, and commands) — see that module's file for details:
//
//   util           shared helpers (login-shell PATH, epoch formatting, base64)
//   db             SQL connections, queries, ER diagram, SQL export/import
//   redis_db       Redis browser
//   pty            terminal tabs (real OS pseudo-terminals)
//   filesystem     local file explorer operations
//   ai             AI chat backend (Claude/OpenAI/Gemini/Ollama/etc.)
//   remote         FTP/SFTP browsing and transfers
//   git            git integration (shells out to the `git` binary)
//   containers     Podman/Docker container & image management
//   linting        quick syntax linting via each language's own toolchain
//   http_client     generic HTTP request panel
//   lsp            Language Server Protocol bridge (IntelliSense)

mod util;
mod db;
mod redis_db;
mod pty;
mod filesystem;
mod ai;
mod remote;
mod git;
mod containers;
mod linting;
mod http_client;
mod lsp;

use db::*;
use redis_db::*;
use pty::*;
use filesystem::*;
use ai::*;
use remote::*;
use git::*;
use containers::*;
use linting::*;
use http_client::*;
use lsp::*;

// Running via `npm run tauri dev` attaches this process to the terminal's
// controlling tty and foreground process group — so an accidental Ctrl+Z (or
// any other trigger of SIGTSTP/SIGTTIN/SIGTTOU) in that terminal suspends the
// whole GUI app at the OS level: not a hang, but literally paused and
// unscheduled, which looks identical to a freeze and can't be dismissed from
// the window itself. A packaged/production build normally has no controlling
// terminal at all, so this can't happen there — but ignoring these signals
// here removes the failure mode in dev too, at zero cost (SIGSTOP still works
// for anyone deliberately pausing the process; only the terminal-triggered
// variants are affected).
#[cfg(unix)]
fn ignore_terminal_stop_signals() {
    unsafe {
        libc::signal(libc::SIGTSTP, libc::SIG_IGN);
        libc::signal(libc::SIGTTIN, libc::SIG_IGN);
        libc::signal(libc::SIGTTOU, libc::SIG_IGN);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(unix)]
    ignore_terminal_stop_signals();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            open_file,
            save_file,
            list_dir,
            get_home_dir,
            path_join,
            create_file,
            write_config_file,
            create_dir_cmd,
            delete_entry,
            rename_entry,
            read_image_base64,
            call_ai,
            list_ollama_models,
            list_openrouter_free_models,
            pty_create,
            pty_write,
            pty_resize,
            pty_kill,
            move_to_trash,
            search_in_files,
            open_in_file_manager,
            db_connect,
            db_create_database,
            db_disconnect,
            db_list_tables,
            db_query,
            db_execute,
            db_describe_table,
            db_export,
            db_import,
            db_get_er_schema,
            db_add_relationship,
            db_drop_relationship,
            hash_value,
            redis_connect,
            redis_disconnect,
            redis_scan_keys,
            redis_db_size,
            redis_get_value,
            redis_set_string,
            redis_delete_key,
            redis_rename_key,
            redis_set_ttl,
            redis_hash_set,
            redis_hash_del,
            redis_list_push,
            redis_list_set,
            redis_list_remove_index,
            redis_set_add,
            redis_set_remove,
            redis_zset_add,
            redis_zset_remove,
            redis_create_key,
            remote_connect,
            remote_disconnect,
            remote_list_dir,
            remote_read_file,
            remote_write_file,
            remote_download_file,
            remote_upload_file,
            remote_delete,
            remote_mkdir,
            remote_rename,
            git_status,
            git_diff,
            git_stage,
            git_unstage,
            git_discard,
            git_clean_untracked,
            git_commit,
            git_push,
            git_pull,
            git_fetch,
            git_branches,
            git_checkout_branch,
            git_create_branch,
            git_log,
            git_remote_url,
            git_init,
            git_config_get,
            git_config_set,
            container_runtime,
            container_list,
            container_start,
            container_stop,
            container_restart,
            container_remove,
            container_export,
            container_logs,
            container_create,
            container_update,
            container_inspect,
            image_list,
            image_pull,
            network_ensure,
            lint_file,
            http_request,
            lsp_start,
            lsp_complete,
            lsp_signature_help,
            lsp_diagnostics,
            lsp_notify_open,
            lsp_stop,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
