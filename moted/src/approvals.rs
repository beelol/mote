use mote_protocol::{ActionDigest, HostId};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserEffect {
    Submit,
    Purchase,
    Message,
    Download,
    OAuth,
    NonGet,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ApprovalIntent {
    Browser {
        origin: String,
        node_ref: String,
        request_fingerprint: ActionDigest,
        effect: BrowserEffect,
        action: Value,
    },
}

impl ApprovalIntent {
    pub fn action_hash(&self) -> Result<ActionDigest, serde_json::Error> {
        fn canonical(value: Value) -> Value {
            match value {
                Value::Object(fields) => {
                    let sorted: std::collections::BTreeMap<_, _> = fields
                        .into_iter()
                        .map(|(key, value)| (key, canonical(value)))
                        .collect();
                    Value::Object(sorted.into_iter().collect())
                }
                Value::Array(items) => Value::Array(items.into_iter().map(canonical).collect()),
                value => value,
            }
        }
        let bytes = serde_json::to_vec(&canonical(serde_json::to_value(self)?))?;
        Ok(ActionDigest::new(format!("{:x}", Sha256::digest(bytes))).expect("SHA-256 digest"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalStatus {
    Pending,
    Claimed,
    Approved,
    Failed,
    Denied,
    Expired,
}

impl ApprovalStatus {
    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "claimed" => Ok(Self::Claimed),
            "approved" => Ok(Self::Approved),
            "failed" => Ok(Self::Failed),
            "denied" => Ok(Self::Denied),
            "expired" => Ok(Self::Expired),
            _ => Err(rusqlite::Error::InvalidQuery),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ApprovalRecord {
    pub id: i64,
    pub host_id: HostId,
    pub thread_id: String,
    pub kind: String,
    pub origin: String,
    pub node_ref: String,
    pub request_fingerprint: ActionDigest,
    pub display: Value,
    pub action_hash: ActionDigest,
    pub page_state_hash: ActionDigest,
    pub status: ApprovalStatus,
    pub expires_at: i64,
    pub created_at: i64,
    pub claimed_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub result: Option<String>,
    pub hrns_action_id: Option<String>,
    pub hrns_request_hash: Option<String>,
}

impl ApprovalRecord {
    pub fn card(&self) -> Value {
        let site = self
            .display
            .get("site")
            .and_then(Value::as_str)
            .unwrap_or("site");
        let effect = self
            .display
            .get("effect")
            .and_then(Value::as_str)
            .unwrap_or("outward action");
        let mut children = vec![
            json!({"type":"text","value":format!("Approve {effect} on {site}? Expires at {}.", self.expires_at)}),
        ];
        if self.status == ApprovalStatus::Pending {
            children.push(json!({"type":"row","children":[
                {"type":"action","label":"Approve","event":format!("approve:{}",self.id)},
                {"type":"action","label":"Deny","event":format!("deny:{}",self.id)}
            ]}));
        } else {
            children.push(json!({"type":"text","value":format!("{:?}",self.status)}));
        }
        json!({"type":"stack","children":children})
    }
}

#[derive(Clone, Debug)]
pub struct ApprovalDecision {
    pub id: i64,
    pub host_id: HostId,
    pub thread_id: String,
    pub action_hash: ActionDigest,
    pub page_state_hash: ActionDigest,
    pub expires_at: i64,
}

pub struct ApprovalClaim {
    record: ApprovalRecord,
    claimed_at: i64,
}

impl ApprovalClaim {
    pub fn record(&self) -> &ApprovalRecord {
        &self.record
    }
}

pub trait ApprovalExecutor {
    fn execute(&mut self, record: &ApprovalRecord, intent: &ApprovalIntent) -> Result<(), String>;
}

pub struct ApprovalStore<'a> {
    conn: &'a Connection,
    host_id: HostId,
}

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS approvals(
        id INTEGER PRIMARY KEY AUTOINCREMENT,thread_id TEXT NOT NULL,kind TEXT NOT NULL,payload TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'pending',expires_at INTEGER NOT NULL,created_at INTEGER NOT NULL)")?;
    let columns: Vec<String> = conn
        .prepare("PRAGMA table_info(approvals)")?
        .query_map([], |row| row.get(1))?
        .collect::<Result<_, _>>()?;
    for (name, ty) in [
        ("action_hash", "TEXT"),
        ("host_id", "TEXT"),
        ("origin", "TEXT"),
        ("node_ref", "TEXT"),
        ("request_fingerprint", "TEXT"),
        ("page_state_hash", "TEXT"),
        ("claimed_at", "INTEGER"),
        ("completed_at", "INTEGER"),
        ("result", "TEXT"),
        ("hrns_action_id", "TEXT"),
        ("hrns_request_hash", "TEXT"),
    ] {
        if !columns.iter().any(|column| column == name) {
            conn.execute_batch(&format!("ALTER TABLE approvals ADD COLUMN {name} {ty}"))?;
        }
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS approvals_host_pending ON approvals(host_id,status,id);
         CREATE UNIQUE INDEX IF NOT EXISTS approvals_hrns_action
         ON approvals(host_id,hrns_action_id) WHERE hrns_action_id IS NOT NULL",
    )?;
    Ok(())
}

fn database_json(error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}

fn redact_json(display: Value) -> Value {
    fn visit(value: Value) -> Value {
        match value {
            Value::String(text) => Value::String(mote_redact::redact(&text)),
            Value::Array(values) => Value::Array(values.into_iter().map(visit).collect()),
            Value::Object(values) => Value::Object(
                values
                    .into_iter()
                    .map(|(key, value)| (key, visit(value)))
                    .collect(),
            ),
            value => value,
        }
    }
    let redacted = mote_redact::redact(&display.to_string());
    visit(serde_json::from_str(&redacted).expect("redacted JSON"))
}

impl<'a> ApprovalStore<'a> {
    pub fn new(conn: &'a Connection, host_id: HostId) -> rusqlite::Result<Self> {
        migrate(conn)?;
        Ok(Self { conn, host_id })
    }

    pub fn create(
        &self,
        thread_id: &str,
        intent: ApprovalIntent,
        page_state_hash: ActionDigest,
        _display: Value,
        now: i64,
        expires_at: i64,
    ) -> rusqlite::Result<ApprovalRecord> {
        self.create_impl(
            thread_id,
            intent,
            page_state_hash,
            now,
            expires_at,
            None,
        )
    }

    pub fn create_for_action(
        &self,
        thread_id: &str,
        intent: ApprovalIntent,
        page_state_hash: ActionDigest,
        now: i64,
        expires_at: i64,
        action: (&str, &str),
    ) -> rusqlite::Result<ApprovalRecord> {
        let (action_id, request_hash) = action;
        if action_id.trim().is_empty()
            || request_hash.len() != 64
            || !request_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(rusqlite::Error::InvalidQuery);
        }
        self.create_impl(
            thread_id,
            intent,
            page_state_hash,
            now,
            expires_at,
            Some((action_id, request_hash)),
        )
    }

    fn create_impl(
        &self,
        thread_id: &str,
        intent: ApprovalIntent,
        page_state_hash: ActionDigest,
        now: i64,
        expires_at: i64,
        action: Option<(&str, &str)>,
    ) -> rusqlite::Result<ApprovalRecord> {
        if thread_id.is_empty() || expires_at <= now {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let action_hash = intent.action_hash().map_err(database_json)?;
        let ApprovalIntent::Browser {
            origin,
            node_ref,
            request_fingerprint,
            effect,
            ..
        } = &intent;
        if origin.is_empty()
            || node_ref.is_empty()
            || origin.contains(['?', '#', '@'])
            || mote_redact::redact(origin) != *origin
            || mote_redact::redact(node_ref) != *node_ref
        {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let display = redact_json(json!({"site":origin,"effect":format!("{effect:?}")}));
        if let Some((action_id, request_hash)) = action {
            if let Some(existing) = self.record_for_action(action_id)? {
                if existing.thread_id == thread_id
                    && existing.action_hash == action_hash
                    && existing.page_state_hash == page_state_hash
                    && existing.hrns_request_hash.as_deref() == Some(request_hash)
                {
                    return Ok(existing);
                }
                return Err(rusqlite::Error::InvalidQuery);
            }
            self.conn.execute("INSERT INTO approvals(host_id,thread_id,kind,payload,status,expires_at,created_at,action_hash,origin,node_ref,request_fingerprint,page_state_hash,hrns_action_id,hrns_request_hash)
                VALUES(?1,?2,'browser',?3,'pending',?4,?5,?6,?7,?8,?9,?10,?11,?12)",rusqlite::params![self.host_id.as_str(),thread_id,display.to_string(),expires_at,now,action_hash.as_str(),origin,node_ref,request_fingerprint.as_str(),page_state_hash.as_str(),action_id,request_hash])?;
        } else {
            self.conn.execute("INSERT INTO approvals(host_id,thread_id,kind,payload,status,expires_at,created_at,action_hash,origin,node_ref,request_fingerprint,page_state_hash)
                VALUES(?1,?2,'browser',?3,'pending',?4,?5,?6,?7,?8,?9,?10)",rusqlite::params![self.host_id.as_str(),thread_id,display.to_string(),expires_at,now,action_hash.as_str(),origin,node_ref,request_fingerprint.as_str(),page_state_hash.as_str()])?;
        }
        self.record(self.conn.last_insert_rowid())?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn record(&self, id: i64) -> rusqlite::Result<Option<ApprovalRecord>> {
        self.conn.query_row("SELECT id,host_id,thread_id,kind,payload,action_hash,page_state_hash,status,expires_at,created_at,claimed_at,completed_at,result,origin,node_ref,request_fingerprint,hrns_action_id,hrns_request_hash
            FROM approvals WHERE id=?1 AND host_id=?2 AND kind='browser'",rusqlite::params![id,self.host_id.as_str()], |row| {
            let invalid = |_| rusqlite::Error::InvalidQuery;
            Ok(ApprovalRecord { id: row.get(0)?,host_id: HostId::new(row.get::<_,String>(1)?).map_err(invalid)?,thread_id:row.get(2)?,kind:row.get(3)?,display:serde_json::from_str(&row.get::<_,String>(4)?).map_err(database_json)?,action_hash:ActionDigest::new(row.get::<_,String>(5)?).map_err(invalid)?,page_state_hash:ActionDigest::new(row.get::<_,String>(6)?).map_err(invalid)?,status:ApprovalStatus::parse(&row.get::<_,String>(7)?)?,expires_at:row.get(8)?,created_at:row.get(9)?,claimed_at:row.get(10)?,completed_at:row.get(11)?,result:row.get(12)?,origin:row.get(13)?,node_ref:row.get(14)?,request_fingerprint:ActionDigest::new(row.get::<_,String>(15)?).map_err(invalid)?,hrns_action_id:row.get(16)?,hrns_request_hash:row.get(17)? })
        }).optional()
    }

    pub fn record_for_action(&self, action_id: &str) -> rusqlite::Result<Option<ApprovalRecord>> {
        let id = self
            .conn
            .query_row(
                "SELECT id FROM approvals WHERE host_id=?1 AND hrns_action_id=?2",
                rusqlite::params![self.host_id.as_str(), action_id],
                |row| row.get(0),
            )
            .optional()?;
        id.map(|id| {
            self.record(id)?
                .ok_or(rusqlite::Error::QueryReturnedNoRows)
        })
        .transpose()
    }

    fn matches(&self, decision: &ApprovalDecision) -> rusqlite::Result<Option<ApprovalRecord>> {
        if decision.host_id != self.host_id {
            return Ok(None);
        }
        Ok(self.record(decision.id)?.filter(|record| {
            record.thread_id == decision.thread_id
                && record.action_hash == decision.action_hash
                && record.page_state_hash == decision.page_state_hash
                && record.expires_at == decision.expires_at
        }))
    }

    pub fn claim(
        &self,
        decision: &ApprovalDecision,
        current_intent: &ApprovalIntent,
        current_page: &ActionDigest,
        now: i64,
    ) -> rusqlite::Result<Option<ApprovalClaim>> {
        self.expire(now)?;
        let Some(record) = self.matches(decision)? else {
            return Ok(None);
        };
        if record.status != ApprovalStatus::Pending
            || record.action_hash != current_intent.action_hash().map_err(database_json)?
            || record.page_state_hash != *current_page
        {
            return Ok(None);
        }
        let changed = self.conn.execute("UPDATE approvals SET status='claimed',claimed_at=?1 WHERE id=?2 AND host_id=?3 AND thread_id=?4 AND action_hash=?5 AND page_state_hash=?6 AND expires_at=?7 AND expires_at>=?1 AND status='pending'",rusqlite::params![now,record.id,self.host_id.as_str(),record.thread_id,record.action_hash.as_str(),record.page_state_hash.as_str(),record.expires_at])?;
        if changed == 0 {
            return Ok(None);
        }
        Ok(Some(ApprovalClaim {
            record: self
                .record(record.id)?
                .ok_or(rusqlite::Error::QueryReturnedNoRows)?,
            claimed_at: now,
        }))
    }

    pub fn complete(
        &self,
        claim: ApprovalClaim,
        outcome: Result<(), String>,
        now: i64,
    ) -> rusqlite::Result<ApprovalRecord> {
        let (status, result) = match outcome {
            Ok(()) => ("approved", None),
            Err(reason) => ("failed", Some(mote_redact::redact(&reason))),
        };
        let changed = self.conn.execute("UPDATE approvals SET status=?1,completed_at=?2,result=?3 WHERE id=?4 AND host_id=?5 AND action_hash=?6 AND claimed_at=?7 AND status='claimed'",rusqlite::params![status,now,result,claim.record.id,self.host_id.as_str(),claim.record.action_hash.as_str(),claim.claimed_at])?;
        if changed != 1 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        self.record(claim.record.id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn decide(
        &self,
        decision: &ApprovalDecision,
        current_intent: &ApprovalIntent,
        current_page: &ActionDigest,
        now: i64,
        executor: &mut impl ApprovalExecutor,
    ) -> rusqlite::Result<ApprovalRecord> {
        if let Some(claim) = self.claim(decision, current_intent, current_page, now)? {
            let outcome = executor.execute(&claim.record, current_intent);
            self.complete(claim, outcome, now)
        } else {
            self.matches(decision)?
                .ok_or(rusqlite::Error::QueryReturnedNoRows)
        }
    }

    pub fn deny(&self, decision: &ApprovalDecision, now: i64) -> rusqlite::Result<bool> {
        self.expire(now)?;
        let Some(record) = self.matches(decision)? else {
            return Ok(false);
        };
        Ok(self.conn.execute("UPDATE approvals SET status='denied',completed_at=?1 WHERE id=?2 AND host_id=?3 AND thread_id=?4 AND action_hash=?5 AND page_state_hash=?6 AND expires_at=?7 AND status='pending' AND expires_at>=?1",rusqlite::params![now,record.id,self.host_id.as_str(),record.thread_id,record.action_hash.as_str(),record.page_state_hash.as_str(),record.expires_at])? == 1)
    }

    pub fn expire(&self, now: i64) -> rusqlite::Result<usize> {
        self.conn.execute("UPDATE approvals SET status='expired',completed_at=?1 WHERE host_id=?2 AND status='pending' AND expires_at<?1",rusqlite::params![now,self.host_id.as_str()])
    }

    pub fn pending_for_host(&self, now: i64) -> rusqlite::Result<Vec<ApprovalRecord>> {
        self.expire(now)?;
        let ids: Vec<i64> = self.conn.prepare("SELECT id FROM approvals WHERE host_id=?1 AND kind='browser' AND status='pending' ORDER BY id")?.query_map([self.host_id.as_str()],|row| row.get(0))?.collect::<Result<_,_>>()?;
        ids.into_iter()
            .map(|id| self.record(id)?.ok_or(rusqlite::Error::QueryReturnedNoRows))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Barrier};

    fn host(value: &str) -> HostId {
        HostId::from_public_key(value.as_bytes())
    }
    fn intent() -> ApprovalIntent {
        ApprovalIntent::Browser {
            origin: "http://127.0.0.1:8080".into(),
            node_ref: "document1:submit".into(),
            request_fingerprint: ActionDigest::new("a".repeat(64)).unwrap(),
            effect: BrowserEffect::Submit,
            action: json!({"method":"POST","body":{"token":"secret-value","text":"hello"}}),
        }
    }
    fn page() -> ActionDigest {
        ActionDigest::new("b".repeat(64)).unwrap()
    }
    fn open_store(conn: &Connection) -> ApprovalStore<'_> {
        ApprovalStore::new(conn, host("home")).unwrap()
    }
    fn create(store: &ApprovalStore<'_>) -> ApprovalRecord {
        store
            .create(
                "thread",
                intent(),
                page(),
                json!({"site":"http://127.0.0.1:8080","effect":"Submit","token":"secret-value"}),
                100,
                200,
            )
            .unwrap()
    }
    fn decision(record: &ApprovalRecord) -> ApprovalDecision {
        ApprovalDecision {
            id: record.id,
            host_id: record.host_id.clone(),
            thread_id: record.thread_id.clone(),
            action_hash: record.action_hash.clone(),
            page_state_hash: record.page_state_hash.clone(),
            expires_at: record.expires_at,
        }
    }

    #[test]
    fn mismatched_host_thread_action_page_and_expiry_cannot_claim() {
        let conn = Connection::open_in_memory().unwrap();
        let store = open_store(&conn);
        let record = create(&store);
        let original = decision(&record);
        for field in 0..5 {
            let mut altered = original.clone();
            match field {
                0 => altered.host_id = host("office"),
                1 => altered.thread_id = "other".into(),
                2 => altered.action_hash = ActionDigest::new("c".repeat(64)).unwrap(),
                3 => altered.page_state_hash = ActionDigest::new("c".repeat(64)).unwrap(),
                _ => altered.expires_at = 201,
            }
            assert!(store
                .claim(&altered, &intent(), &page(), 101)
                .unwrap()
                .is_none());
        }
        let mut changed = intent();
        let ApprovalIntent::Browser { action, .. } = &mut changed;
        *action = json!({"method":"DELETE"});
        assert!(store
            .claim(&original, &changed, &page(), 101)
            .unwrap()
            .is_none());
        assert!(store
            .claim(
                &original,
                &intent(),
                &ActionDigest::new("d".repeat(64)).unwrap(),
                101
            )
            .unwrap()
            .is_none());
        assert!(ApprovalStore::new(&conn, host("office"))
            .unwrap()
            .pending_for_host(101)
            .unwrap()
            .is_empty());
        assert_eq!(
            store.record(record.id).unwrap().unwrap().status,
            ApprovalStatus::Pending
        );
    }

    #[test]
    fn display_and_database_contain_no_executable_intent() {
        let conn = Connection::open_in_memory().unwrap();
        let store = open_store(&conn);
        let record = create(&store);
        let payload: String = conn
            .query_row(
                "SELECT payload FROM approvals WHERE id=?1",
                [record.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!payload.contains("secret-value"));
        assert!(!payload.contains("POST"));
        let card = record.card().to_string();
        assert!(card.contains("Submit"));
        assert!(card.contains("200"));
        assert!(!card.contains("secret-value"));
        assert!(!card.contains("method"));
        mote_ui::validate(record.card().to_string().as_bytes()).unwrap();
    }

    #[test]
    fn card_effect_is_derived_from_exact_intent() {
        let conn = Connection::open_in_memory().unwrap();
        let store = open_store(&conn);
        let record = store
            .create(
                "thread",
                intent(),
                page(),
                json!({"site":"https://attacker.invalid","effect":"Read"}),
                100,
                200,
            )
            .unwrap();
        assert_eq!(record.display["effect"], "Submit");
        assert_eq!(record.display["site"], "http://127.0.0.1:8080");
    }

    #[test]
    fn mismatched_deny_never_changes_pending_state() {
        let conn = Connection::open_in_memory().unwrap();
        let store = open_store(&conn);
        let record = create(&store);
        for field in 0..5 {
            let mut altered = decision(&record);
            match field {
                0 => altered.host_id = host("office"),
                1 => altered.thread_id = "other".into(),
                2 => altered.action_hash = ActionDigest::new("c".repeat(64)).unwrap(),
                3 => altered.page_state_hash = ActionDigest::new("c".repeat(64)).unwrap(),
                _ => altered.expires_at = 201,
            }
            assert!(!store.deny(&altered, 101).unwrap());
        }
        assert_eq!(
            store.record(record.id).unwrap().unwrap().status,
            ApprovalStatus::Pending
        );
    }

    #[test]
    fn canonical_hash_is_independent_of_object_order_and_binds_all_metadata() {
        let original = intent();
        let mut reordered = intent();
        let ApprovalIntent::Browser { action, .. } = &mut reordered;
        *action = serde_json::from_str(
            r#"{"body":{"text":"hello","token":"secret-value"},"method":"POST"}"#,
        )
        .unwrap();
        assert_eq!(
            original.action_hash().unwrap(),
            reordered.action_hash().unwrap()
        );
        for field in 0..4 {
            let mut changed = intent();
            let ApprovalIntent::Browser {
                origin,
                node_ref,
                request_fingerprint,
                effect,
                ..
            } = &mut changed;
            match field {
                0 => *origin = "http://127.0.0.1:8081".into(),
                1 => *node_ref = "document2:submit".into(),
                2 => *request_fingerprint = ActionDigest::new("f".repeat(64)).unwrap(),
                _ => *effect = BrowserEffect::Purchase,
            }
            assert_ne!(
                original.action_hash().unwrap(),
                changed.action_hash().unwrap()
            );
        }
    }

    #[test]
    fn hrns_action_binding_is_idempotent_and_rejects_altered_requests() {
        let conn = Connection::open_in_memory().unwrap();
        let store = open_store(&conn);
        let first = store
            .create_for_action(
                "thread",
                intent(),
                page(),
                100,
                200,
                ("task:1", &"c".repeat(64)),
            )
            .unwrap();
        let repeated = store
            .create_for_action(
                "thread",
                intent(),
                page(),
                100,
                200,
                ("task:1", &"c".repeat(64)),
            )
            .unwrap();
        assert_eq!(repeated.id, first.id);
        assert_eq!(repeated.hrns_action_id.as_deref(), Some("task:1"));
        assert_eq!(
            repeated.hrns_request_hash.as_deref(),
            Some("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
        );
        assert!(store
            .create_for_action(
                "thread",
                intent(),
                page(),
                100,
                200,
                ("task:1", &"d".repeat(64)),
            )
            .is_err());
        assert_eq!(store.pending_for_host(101).unwrap().len(), 1);
    }

    #[test]
    fn deny_expiry_and_duplicate_decisions_are_inert() {
        let conn = Connection::open_in_memory().unwrap();
        let store = open_store(&conn);
        let denied = create(&store);
        assert!(store.deny(&decision(&denied), 101).unwrap());
        assert!(!store.deny(&decision(&denied), 102).unwrap());
        assert!(store
            .claim(&decision(&denied), &intent(), &page(), 102)
            .unwrap()
            .is_none());
        let expired = create(&store);
        assert!(store
            .claim(&decision(&expired), &intent(), &page(), 201)
            .unwrap()
            .is_none());
        assert_eq!(
            store.record(expired.id).unwrap().unwrap().status,
            ApprovalStatus::Expired
        );
        assert_eq!(
            store.record(denied.id).unwrap().unwrap().status,
            ApprovalStatus::Denied
        );
    }

    struct Counter<'a>(&'a Connection);
    impl ApprovalExecutor for Counter<'_> {
        fn execute(
            &mut self,
            _record: &ApprovalRecord,
            _intent: &ApprovalIntent,
        ) -> Result<(), String> {
            self.0
                .execute("UPDATE effects SET count=count+1", [])
                .map_err(|e| e.to_string())?;
            Ok(())
        }
    }

    #[test]
    fn one_effect_survives_repeated_decisions_and_restart_after_claim() {
        let dir = std::env::temp_dir().join(format!(
            "mote-approval-durable-{}-{}",
            std::process::id(),
            crate::now_ms()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mote.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE effects(count INTEGER); INSERT INTO effects VALUES(0)")
            .unwrap();
        let store = open_store(&conn);
        let record = create(&store);
        let mut executor = Counter(&conn);
        assert_eq!(
            store
                .decide(&decision(&record), &intent(), &page(), 101, &mut executor)
                .unwrap()
                .status,
            ApprovalStatus::Approved
        );
        assert_eq!(
            store
                .decide(&decision(&record), &intent(), &page(), 102, &mut executor)
                .unwrap()
                .status,
            ApprovalStatus::Approved
        );
        let ambiguous = create(&store);
        let claim = store
            .claim(&decision(&ambiguous), &intent(), &page(), 101)
            .unwrap()
            .unwrap();
        executor.execute(&claim.record, &intent()).unwrap();
        drop(conn);
        let conn = Connection::open(&path).unwrap();
        let store = open_store(&conn);
        assert_eq!(
            store
                .decide(
                    &decision(&ambiguous),
                    &intent(),
                    &page(),
                    102,
                    &mut Counter(&conn)
                )
                .unwrap()
                .status,
            ApprovalStatus::Claimed
        );
        let count: i64 = conn
            .query_row("SELECT count FROM effects", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
        drop(conn);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn concurrent_claim_has_one_winner() {
        let path = std::env::temp_dir().join(format!(
            "mote-approval-concurrent-{}-{}.db",
            std::process::id(),
            crate::now_ms()
        ));
        let conn = Connection::open(&path).unwrap();
        let record = create(&open_store(&conn));
        let barrier = Arc::new(Barrier::new(2));
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let path = path.clone();
                let decision = decision(&record);
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let conn = Connection::open(path).unwrap();
                    let store = open_store(&conn);
                    barrier.wait();
                    store
                        .claim(&decision, &intent(), &page(), 101)
                        .unwrap()
                        .is_some()
                })
            })
            .collect();
        assert_eq!(
            workers
                .into_iter()
                .map(|worker| worker.join().unwrap())
                .filter(|won| *won)
                .count(),
            1
        );
        drop(conn);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn executor_failure_is_terminal_and_not_retried() {
        struct Failure;
        impl ApprovalExecutor for Failure {
            fn execute(&mut self, _: &ApprovalRecord, _: &ApprovalIntent) -> Result<(), String> {
                Err("sk-abcdefghijklmnopqrstuv".into())
            }
        }
        let conn = Connection::open_in_memory().unwrap();
        let store = open_store(&conn);
        let record = create(&store);
        let result = store
            .decide(&decision(&record), &intent(), &page(), 101, &mut Failure)
            .unwrap();
        assert_eq!(result.status, ApprovalStatus::Failed);
        assert!(store
            .claim(&decision(&record), &intent(), &page(), 102)
            .unwrap()
            .is_none());
        assert!(!result.result.unwrap().contains("sk-abcdefghijklmnopqrstuv"));
    }
}
