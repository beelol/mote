//! Opt-in real-daemon fixture for external integration and Simulator tests.

use super::*;
use mote_testkit::gateway::{FixtureResult, LoopbackHost};
use mote_testkit::ScriptedServer;

/// Starts the real dispatcher and encrypted gateway using an isolated scripted model server.
pub fn start(models: &ScriptedServer) -> FixtureResult<LoopbackHost<impl Dispatch>> {
    LoopbackHost::start(|data| build(data, models))
}

pub(super) fn build(
    data: &Path,
    models: &ScriptedServer,
) -> FixtureResult<(Arc<Daemon>, EventLog)> {
    let identity = gateway::prepare_identity(data).map_err(std::io::Error::other)?;
    let db = data.join("mote.db");
    let log = EventLog::open(&db)?;
    let config = ConfigStore::load(
        &data.join("config.toml"),
        log.clone(),
        Arc::new(OllamaGate::new(Ollama::new(models.base_url()))),
    )
    .map_err(|error| std::io::Error::other(error.to_string()))?;
    let daemon = Daemon {
        host_id: identity.host_id().clone(),
        gateway: Mutex::new(None),
        log: log.clone(),
        config: Mutex::new(config),
        character: Arc::new(Mutex::new(Charter {
            front: charter::FrontMatter {
                name: "mote".into(),
                tools: Vec::new(),
                realms: vec!["personal".into()],
            },
            prose: String::new(),
        })),
        store: Mutex::new(ContextStore::open(&db)?),
        client: Ollama::new(models.base_url()),
        watchers: Mutex::new(Watchers::open(rusqlite::Connection::open(&db)?)?),
        think: Mutex::new(ThinkServers::new(data.to_path_buf())),
        matrix_sources: Mutex::new(HashMap::new()),
        matrix_snapshots: Mutex::new(HashMap::new()),
        matrix_surfaces: Mutex::new(HashMap::new()),
        matrix_candidates: Mutex::new(HashMap::new()),
        inferred_retry_budget: Mutex::new(inferred_ui::RetryBudget::default()),
        inferred_surface_turns: Mutex::new(HashMap::new()),
        data_dir: data.to_path_buf(),
        started: std::time::Instant::now(),
    };
    Ok((Arc::new(daemon), log))
}
