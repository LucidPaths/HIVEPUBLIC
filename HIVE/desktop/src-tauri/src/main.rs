//! HIVE Desktop - Local LLM Runtime
//!
//! Modular architecture:
//!   types.rs       — Shared data structures
//!   state.rs       — Tauri-managed runtime state
//!   paths.rs       — App data directory helpers
//!   http_client.rs — Shared HTTP client with User-Agent
//!   wsl.rs         — WSL2 bridge (detection, path conversion, commands)
//!   security.rs    — AES-256-GCM encrypted API key & hardware data storage
//!   hardware.rs    — GPU/CPU/RAM detection + dependency checks
//!   models.rs      — Local & WSL model listing
//!   server.rs      — llama-server lifecycle management
//!   download.rs    — Streaming model downloads (Windows + WSL)
//!   gguf.rs        — GGUF file parsing + VRAM estimation
//!   providers.rs   — Chat providers (Local, Ollama, OpenAI, Anthropic)
//!   tools/         — MCP-compatible tool framework (read, write, exec, web)
//!   memory.rs      — Persistent memory system (SQLite + FTS5 + Markdown)

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;

/// P6: Global flag for minimize-to-tray behavior.
/// Set by the frontend via set_minimize_to_tray command.
static MINIMIZE_TO_TRAY: AtomicBool = AtomicBool::new(false);

mod types;
mod state;
mod paths;
mod http_client;
mod wsl;
mod security;
mod content_security;
mod hardware;
mod models;
mod server;
mod download;
mod gguf;
mod providers;
mod provider_tools;
mod provider_stream;
mod tools;
mod memory;
mod magma;
mod working_memory;
mod harness;
mod slots;
mod orchestrator;
mod telegram_daemon;
mod discord_daemon;
mod routines;
mod mcp_server;
mod mcp_client;
mod pty_manager;
mod tunnel;

/// P6: Toggle minimize-to-tray behavior from the frontend.
#[tauri::command]
fn set_minimize_to_tray(enabled: bool) {
    MINIMIZE_TO_TRAY.store(enabled, Ordering::Relaxed);
    tools::log_tools::append_to_app_log(
        &format!("HIVE | tray | minimize_to_tray set to {}", enabled)
    );
}

/// P6: Get current minimize-to-tray state (for frontend sync on startup).
#[tauri::command]
fn get_minimize_to_tray() -> bool {
    MINIMIZE_TO_TRAY.load(Ordering::Relaxed)
}

/// Shared cleanup: kill all servers, PTY sessions, and signal daemons to stop.
/// Called from both tray quit handler and window close handler (DRY).
fn perform_full_cleanup(app: &tauri::AppHandle) {
    let app_state: tauri::State<'_, state::AppState> = app.state();
    server::cleanup_all_servers(app_state.inner());
    pty_manager::kill_all_sessions();
    let tg: tauri::State<'_, telegram_daemon::TelegramDaemonState> = app.state();
    tg.running.store(false, Ordering::Relaxed);
    let dc: tauri::State<'_, discord_daemon::DiscordDaemonState> = app.state();
    dc.running.store(false, Ordering::Relaxed);
    let rt: tauri::State<'_, routines::RoutinesDaemonState> = app.state();
    rt.running.store(false, Ordering::Relaxed);
    tools::log_tools::append_to_app_log(
        "HIVE | shutdown | Clean exit — all servers, daemons, and PTY sessions terminated"
    );
}

fn main() {
    // MCP server mode: run as headless stdio MCP server (for Claude Code integration)
    // Usage: hive-desktop --mcp
    if std::env::args().any(|a| a == "--mcp") {
        tokio::runtime::Runtime::new()
            .expect("Failed to create tokio runtime")
            .block_on(mcp_server::run());
        return;
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Store global AppHandle so background tasks (workers, PTY) can emit events
            tools::worker_tools::set_app_handle(app.handle().clone());
            pty_manager::set_app_handle(app.handle().clone());

            // P6: System tray — HIVE persists as background daemon when minimized
            let show_item = tauri::menu::MenuItem::with_id(app, "show", "Show HIVE", true, None::<&str>)?;
            let quit_item = tauri::menu::MenuItem::with_id(app, "quit", "Quit HIVE", true, None::<&str>)?;
            let tray_menu = tauri::menu::Menu::with_items(app, &[&show_item, &quit_item])?;

            let _tray = tauri::tray::TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().unwrap_or_else(|| {
                    tools::log_tools::append_to_app_log(
                        "HIVE | warning | No app icon found — using 1x1 fallback (P4)"
                    );
                    tauri::image::Image::new(&[0, 0, 0, 0], 1, 1)
                }))
                .tooltip("HIVE — AI Orchestration")
                .menu(&tray_menu)
                .menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "show" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        "quit" => {
                            perform_full_cleanup(app);
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event {
                        if let Some(w) = tray.app_handle().get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .manage(state::AppState::default())
        .manage(tools::ToolState::default())
        .manage(memory::MemoryState::default())
        .manage(slots::SlotsState::default())
        .manage(telegram_daemon::TelegramDaemonState::default())
        .manage(discord_daemon::DiscordDaemonState::default())
        .manage(routines::RoutinesDaemonState::default())
        .manage(mcp_client::McpClientState::default())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // P6: If minimize-to-tray is enabled, hide instead of exit
                if MINIMIZE_TO_TRAY.load(Ordering::Relaxed) {
                    api.prevent_close();
                    let _ = window.hide();
                    return;
                }

                perform_full_cleanup(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            // Hardware detection
            hardware::detect_gpus,
            hardware::get_system_info,
            hardware::get_live_resource_usage,
            // WSL management
            wsl::check_wsl,
            wsl::run_wsl_command,
            wsl::set_wsl_distro,
            // Dependency management
            hardware::check_dependencies,
            hardware::download_llama_server,
            hardware::get_llama_server_install_path,
            // Model management
            models::list_local_models,
            models::list_wsl_models,
            models::get_models_directory,
            models::open_models_directory,
            // Model download
            download::download_model,
            download::download_model_wsl,
            download::get_remote_file_size,
            // Server management
            server::start_server_native,
            server::start_server_wsl,
            server::stop_server,
            server::get_server_status,
            server::read_server_log,
            // Phase 4: specialist server management
            server::start_specialist_server,
            server::start_specialist_server_wsl,
            server::stop_specialist_server,
            server::get_specialist_servers,
            // App paths + file attachments
            paths::get_app_paths,
            paths::save_attachment,
            // VRAM calculation
            gguf::get_gguf_metadata,
            gguf::estimate_model_vram,
            gguf::check_vram_compatibility,
            // Secure storage (API keys)
            security::store_api_key,
            security::store_api_keys,
            security::has_api_key,
            security::delete_api_key,
            security::get_api_key_count,
            // Encrypted hardware data
            security::store_encrypted_hardware_data,
            security::get_encrypted_hardware_data,
            // Provider management
            providers::get_providers,
            providers::check_provider_status,
            providers::chat_with_provider,
            providers::chat_with_provider_stream,
            providers::chat_with_tools,
            providers::set_session_model_context,
            // Tool framework
            tools::get_available_tools,
            tools::execute_tool,
            tools::log_to_app,
            // Worker status polling (Phase 8 — autonomous sub-agents)
            tools::get_worker_statuses,
            // Context bus (Phase 5C — shared agent activity feed)
            tools::context_bus_write,
            tools::context_bus_summary,
            // Memory system
            memory::memory_init,
            memory::memory_save,
            memory::memory_search,
            memory::memory_list,
            memory::memory_delete,
            memory::memory_clear_all,
            memory::memory_tier_counts,
            memory::memory_promote,
            memory::memory_stats,
            memory::memory_has_embeddings_provider,
            memory::memory_extract_and_save,
            memory::memory_remember,
            memory::memory_recall,
            // Working memory (Phase 3.5 — per-session scratchpad)
            memory::working_memory_read,
            memory::working_memory_write,
            memory::working_memory_append,
            memory::working_memory_clear,
            memory::working_memory_flush,
            // Session handoff notes (Phase 3.5.6 — AI continuity)
            memory::session_notes_read,
            memory::session_notes_write,
            // Cross-session task tracking (Phase 3.5.6)
            memory::memory_task_upsert,
            memory::memory_task_list,
            // Skills as graph nodes (Phase 3.5.5)
            memory::memory_sync_skills,
            memory::memory_discover_skills,
            // Document ingestion / RAG (Phase 9)
            memory::memory_import_file,
            // Markdown ↔ DB sync (Phase 3.5.5 — Obsidian-compatible)
            memory::memory_reimport_markdown,
            memory::memory_get_directory,
            // MAGMA multi-graph (Phase 4)
            memory::magma_add_event,
            memory::magma_events_since,
            memory::magma_upsert_entity,
            memory::magma_get_entity,
            memory::magma_list_entities,
            memory::magma_save_procedure,
            memory::magma_record_procedure_outcome,
            memory::magma_add_edge,
            memory::magma_traverse,
            memory::magma_stats,
            // Slot system (Phase 4)
            slots::get_slot_configs,
            slots::get_slot_states,
            slots::configure_slot,
            slots::add_slot_fallback,
            slots::get_vram_budget,
            slots::set_vram_total,
            // Orchestrator (Phase 4)
            orchestrator::route_task,
            orchestrator::get_wake_context,
            orchestrator::record_slot_wake,
            orchestrator::record_slot_sleep,
            // Cognitive Harness (replaces daemon BIOS Hull)
            harness::harness_build,
            harness::harness_get_identity,
            harness::harness_save_identity,
            harness::harness_reset_identity,
            harness::harness_get_identity_path,
            // Skills (Phase 4.5.5)
            harness::harness_list_skills,
            harness::harness_read_skill,
            harness::harness_get_skills_path,
            harness::harness_open_skills_dir,
            harness::harness_get_relevant_skills,
            // Telegram daemon (Phase 4.5)
            telegram_daemon::start_telegram_daemon,
            telegram_daemon::stop_telegram_daemon,
            telegram_daemon::get_telegram_daemon_status,
            telegram_daemon::set_telegram_host_ids,
            telegram_daemon::set_telegram_user_ids,
            telegram_daemon::get_telegram_access_lists,
            // Discord daemon (Phase 5)
            discord_daemon::start_discord_daemon,
            discord_daemon::stop_discord_daemon,
            discord_daemon::get_discord_daemon_status,
            discord_daemon::set_discord_watched_channels,
            discord_daemon::set_discord_host_ids,
            discord_daemon::set_discord_user_ids,
            discord_daemon::get_discord_access_lists,
            // Routines engine (Phase 6 — Standing Instructions)
            routines::routine_create,
            routines::routine_list,
            routines::routine_update,
            routines::routine_delete,
            routines::routine_record_run,
            routines::routine_stats,
            // Message queue
            routines::queue_enqueue,
            routines::queue_dequeue,
            routines::queue_complete,
            routines::queue_fail,
            routines::queue_status,
            routines::queue_purge_completed,
            // Routines cron daemon
            routines::routines_daemon_start,
            routines::routines_daemon_stop,
            routines::routines_daemon_status,
            // MCP client (consume external MCP servers)
            mcp_client::mcp_connect,
            mcp_client::mcp_disconnect,
            mcp_client::mcp_list_connections,
            // PTY terminal panes (Phase 10 — NEXUS)
            pty_manager::pty_spawn,
            pty_manager::pty_write,
            pty_manager::pty_resize,
            pty_manager::pty_kill,
            pty_manager::pty_list,
            pty_manager::check_agent_available,
            pty_manager::setup_mcp_bridge,
            // P6: System tray
            set_minimize_to_tray,
            get_minimize_to_tray,
            // P7: Cloudflare tunnel for remote access
            tunnel::tunnel_start,
            tunnel::tunnel_stop,
            tunnel::tunnel_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running HIVE application");
}
