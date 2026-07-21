//! SQLite-backed graph store. Schema matches docs/03-graph-data-model.md §6 exactly.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use tracing::debug;

use super::model::{Edge, FlawFlag, Node, NodeType};

pub struct GraphStore {
    conn: Connection,
}

impl GraphStore {
    /// Open (or create) the store at the given path.
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)
            .with_context(|| format!("failed to open SQLite DB at {}", db_path.display()))?;

        // Enable WAL mode for better write concurrency and crash safety.
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

        let store = Self { conn };
        store.run_migrations()?;
        Ok(store)
    }

    /// Create an in-memory store (useful for tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let store = Self { conn };
        store.run_migrations()?;
        Ok(store)
    }

    // -----------------------------------------------------------------------
    // Schema migrations
    // -----------------------------------------------------------------------

    fn run_migrations(&self) -> Result<()> {
        self.conn.execute_batch(SCHEMA_V1)?;
        // Track applied migration version to make future migrations safe.
        // PRAGMA user_version is a 32-bit integer stored in the database file.
        // SCHEMA_V1 already uses IF NOT EXISTS everywhere, making it idempotent.
        // Subsequent migration statements MUST be gated behind a version check.
        debug!("GraphStore schema initialized");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Node operations
    // -----------------------------------------------------------------------

    /// Upsert a node (insert or replace by id).
    pub fn upsert_node(&self, node: &Node) -> Result<()> {
        let base = node.base();
        let now = Utc::now().to_rfc3339();
        let extra_json = serde_json::to_string(node)
            .context("failed to serialize node to extra_json")?;

        let (start_line, end_line) = base
            .span
            .as_ref()
            .map(|s| (Some(s.start_line), Some(s.end_line)))
            .unwrap_or((None, None));

        let (signature, has_incoming_calls, is_entry_point_candidate) = match node {
            Node::Function(f) => (
                Some(f.signature.clone()),
                Some(f.has_incoming_calls as i32),
                Some(f.is_entry_point_candidate as i32),
            ),
            _ => (None, None, None),
        };

        let type_str = match &base.node_type {
            NodeType::Feature => "feature",
            NodeType::Module => "module",
            NodeType::File => "file",
            NodeType::Function => "function",
            NodeType::Type => "type",
            NodeType::Macro => "macro",
            NodeType::Global => "global",
            NodeType::External => "external",
        };

        self.conn.execute(
            "INSERT INTO nodes (
                id, type, name, language, file_path,
                start_line, end_line, signature, extra_json,
                ai_summary, ai_summary_status, ai_model_used,
                structural_hash, has_incoming_calls, is_entry_point_candidate,
                created_at, updated_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?16)
             ON CONFLICT(id) DO UPDATE SET
                name=excluded.name,
                language=excluded.language,
                file_path=excluded.file_path,
                start_line=excluded.start_line,
                end_line=excluded.end_line,
                signature=excluded.signature,
                extra_json=excluded.extra_json,
                ai_summary=excluded.ai_summary,
                ai_summary_status=excluded.ai_summary_status,
                ai_model_used=excluded.ai_model_used,
                structural_hash=excluded.structural_hash,
                has_incoming_calls=excluded.has_incoming_calls,
                is_entry_point_candidate=excluded.is_entry_point_candidate,
                updated_at=excluded.updated_at",
            params![
                base.id,
                type_str,
                base.name,
                base.language,
                base.file_path,
                start_line,
                end_line,
                signature,
                extra_json,
                base.ai_summary,
                ai_summary_status_str(&base.ai_summary_status),
                base.ai_model_used,
                base.structural_hash,
                has_incoming_calls,
                is_entry_point_candidate,
                now,
            ],
        )
        .context("upsert_node failed")?;
        Ok(())
    }

    /// Delete all nodes (and their cascaded edges/flaws) for a given file_path.
    pub fn delete_nodes_for_file(&self, file_path: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM nodes WHERE file_path = ?1",
            params![file_path],
        )?;
        Ok(())
    }

    /// Fetch a node by id. Returns None if not found.
    pub fn get_node(&self, id: &str) -> Result<Option<Node>> {
        let mut stmt = self.conn.prepare(
            "SELECT extra_json, has_incoming_calls, is_entry_point_candidate FROM nodes WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            let json: String = row.get(0)?;
            let has_incoming_calls: Option<i32> = row.get(1)?;
            let is_entry_point_candidate: Option<i32> = row.get(2)?;
            let mut node: Node =
                serde_json::from_str(&json).context("failed to deserialize node")?;
            node.fix_node_type();
            // Patch function-specific columns that may have been updated
            // after the node was first written (e.g. recompute_incoming_calls).
            if let Node::Function(f) = &mut node {
                if let Some(v) = has_incoming_calls {
                    f.has_incoming_calls = v != 0;
                }
                if let Some(v) = is_entry_point_candidate {
                    f.is_entry_point_candidate = v != 0;
                }
            }
            return Ok(Some(node));
        }
        Ok(None)
    }

    /// Return all node ids for a given file_path.
    pub fn node_ids_for_file(&self, file_path: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM nodes WHERE file_path = ?1")?;
        let ids = stmt
            .query_map(params![file_path], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(ids)
    }

    /// Count of all nodes.
    pub fn node_count(&self) -> Result<u64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    // -----------------------------------------------------------------------
    // Edge operations
    // -----------------------------------------------------------------------

    /// Upsert an edge.
    pub fn upsert_edge(&self, edge: &Edge) -> Result<()> {
        let edge_type = match edge.edge_type {
            super::model::EdgeType::Calls => "calls",
            super::model::EdgeType::Includes => "includes",
            super::model::EdgeType::Defines => "defines",
            super::model::EdgeType::References => "references",
            super::model::EdgeType::LinksInto => "links_into",
            super::model::EdgeType::MemberOf => "member_of",
        };
        let confidence = match edge.confidence {
            super::model::EdgeConfidence::Exact => "exact",
            super::model::EdgeConfidence::Probable => "probable",
            super::model::EdgeConfidence::DynamicUnresolved => "dynamic_unresolved",
        };
        self.conn.execute(
            "INSERT INTO edges (id, from_id, to_id, type, confidence, order_hint, condition)
             VALUES (?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(id) DO UPDATE SET
                confidence=excluded.confidence,
                order_hint=excluded.order_hint,
                condition=excluded.condition",
            params![
                edge.id,
                edge.from_id,
                edge.to_id,
                edge_type,
                confidence,
                edge.order_hint,
                edge.condition,
            ],
        )
        .context("upsert_edge failed")?;
        Ok(())
    }

    /// Delete all edges whose from_id belongs to a given file's nodes.
    pub fn delete_edges_for_file(&self, file_path: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM edges WHERE from_id IN (SELECT id FROM nodes WHERE file_path = ?1)",
            params![file_path],
        )?;
        Ok(())
    }

    /// Count of all edges.
    pub fn edge_count(&self) -> Result<u64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    /// Return all edges going out from a node.
    pub fn edges_from(&self, node_id: &str) -> Result<Vec<Edge>> {
        self.query_edges("SELECT * FROM edges WHERE from_id = ?1", node_id)
    }

    /// Return all edges coming into a node.
    pub fn edges_to(&self, node_id: &str) -> Result<Vec<Edge>> {
        self.query_edges("SELECT * FROM edges WHERE to_id = ?1", node_id)
    }

    /// Get a single edge by ID.
    pub fn get_edge(&self, edge_id: &str) -> Result<Option<Edge>> {
        let mut stmt = self.conn.prepare("SELECT * FROM edges WHERE id = ?1")?;
        let mut rows = stmt.query(params![edge_id])?;
        match rows.next()? {
            Some(row) => {
                let edge = edge_from_row(row)?;
                Ok(Some(edge))
            }
            None => Ok(None),
        }
    }

    fn query_edges(&self, sql: &str, id: &str) -> Result<Vec<Edge>> {
        let mut stmt = self.conn.prepare(sql)?;
        let mut rows = stmt.query(params![id])?;
        let mut edges = Vec::new();
        while let Some(row) = rows.next()? {
            let edge = edge_from_row(row).context("edge row deserialization")?;
            edges.push(edge);
        }
        Ok(edges)
    }

    // -----------------------------------------------------------------------
    // Flaw operations
    // -----------------------------------------------------------------------

    /// Upsert a flaw record.
    pub fn upsert_flaw(&self, flaw: &FlawFlag, primary_node_id: &str) -> Result<()> {
        let kind = match flaw.kind {
            super::model::FlawKind::DeadCode => "dead_code",
            super::model::FlawKind::CircularDependency => "circular_dependency",
            super::model::FlawKind::StructuralSmell => "structural_smell",
            super::model::FlawKind::SuspectedBug => "suspected_bug",
            super::model::FlawKind::MissingErrorHandling => "missing_error_handling",
            super::model::FlawKind::ResourceLeakPattern => "resource_leak_pattern",
        };
        let severity = match flaw.severity {
            super::model::FlawSeverity::Info => "info",
            super::model::FlawSeverity::Warning => "warning",
            super::model::FlawSeverity::High => "high",
        };
        let basis = match flaw.basis {
            super::model::FlawBasis::Structural => "structural",
            super::model::FlawBasis::Ai => "ai",
            super::model::FlawBasis::StructuralPlusAi => "structural+ai",
        };
        let related_json = flaw
            .related_node_ids
            .as_ref()
            .map(|ids| serde_json::to_string(ids))
            .transpose()?;

        self.conn.execute(
            "INSERT INTO flaws (id, kind, severity, description, confidence, basis, primary_node_id, related_node_ids_json)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(id) DO UPDATE SET
                severity=excluded.severity,
                description=excluded.description,
                confidence=excluded.confidence,
                basis=excluded.basis",
            params![
                flaw.id,
                kind,
                severity,
                flaw.description,
                flaw.confidence,
                basis,
                primary_node_id,
                related_json,
            ],
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Manifest operations
    // -----------------------------------------------------------------------

    pub fn upsert_manifest_entry(
        &self,
        file_path: &str,
        content_hash: &str,
        language: Option<&str>,
        parse_status: &str,
        parse_error: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO files_manifest (file_path, content_hash, language, last_parsed_at, parse_status, parse_error)
             VALUES (?1,?2,?3,?4,?5,?6)
             ON CONFLICT(file_path) DO UPDATE SET
                content_hash=excluded.content_hash,
                language=excluded.language,
                last_parsed_at=excluded.last_parsed_at,
                parse_status=excluded.parse_status,
                parse_error=excluded.parse_error",
            params![file_path, content_hash, language, now, parse_status, parse_error],
        )?;
        Ok(())
    }

    pub fn get_manifest_hash(&self, file_path: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT content_hash FROM files_manifest WHERE file_path = ?1")?;
        let mut rows = stmt.query(params![file_path])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(row.get(0)?));
        }
        Ok(None)
    }

    pub fn manifest_entry_count(&self) -> Result<u64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM files_manifest", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    pub fn delete_manifest_entry(&self, file_path: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM files_manifest WHERE file_path = ?1",
            params![file_path],
        )?;
        Ok(())
    }

    /// Return all manifest entries — used to rebuild manifest.json from SQLite.
    pub fn all_manifest_entries(
        &self,
    ) -> Result<Vec<(String, crate::graph::incremental::ManifestEntry)>> {
        let mut stmt = self.conn.prepare(
            "SELECT file_path, content_hash, language, last_parsed_at, parse_status
             FROM files_manifest",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (path, hash, lang, parsed_at, status) = row?;
            result.push((
                path,
                crate::graph::incremental::ManifestEntry {
                    content_hash: hash,
                    language: lang,
                    last_parsed_at: parsed_at,
                    parse_status: status,
                },
            ));
        }
        Ok(result)
    }

    // -----------------------------------------------------------------------
    // Query helpers
    // -----------------------------------------------------------------------

    /// Search nodes by name substring match.
    pub fn search_nodes_by_name(&self, query: &str) -> Result<Vec<Node>> {
        let escaped = query.replace('%', "~%").replace('_', "~_");
        let pattern = format!("%{}%", escaped);
        let mut stmt = self.conn.prepare(
            "SELECT extra_json, has_incoming_calls, is_entry_point_candidate
             FROM nodes WHERE name LIKE ?1 ESCAPE '~'
             ORDER BY name LIMIT 50",
        )?;
        let mut rows = stmt.query(params![pattern])?;
        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            let json: String = row.get(0)?;
            let mut node: Node =
                serde_json::from_str(&json).context("deserialize search result")?;
            node.fix_node_type();
            if let Node::Function(f) = &mut node {
                if let Some(v) = row.get::<_, Option<i32>>(1)? {
                    f.has_incoming_calls = v != 0;
                }
                if let Some(v) = row.get::<_, Option<i32>>(2)? {
                    f.is_entry_point_candidate = v != 0;
                }
            }
            results.push(node);
        }
        Ok(results)
    }

    /// Search nodes by name, file_path, or ai_summary (text search).
    pub fn search_nodes_by_text(&self, query: &str) -> Result<Vec<Node>> {
        let escaped = query.replace('%', "~%").replace('_', "~_");
        let pattern = format!("%{}%", escaped);
        let mut stmt = self.conn.prepare(
            "SELECT extra_json, has_incoming_calls, is_entry_point_candidate
             FROM nodes
             WHERE name LIKE ?1 ESCAPE '~'
                OR file_path LIKE ?1 ESCAPE '~'
                OR (ai_summary LIKE ?1 ESCAPE '~' AND ai_summary_status = 'ready')
             ORDER BY name LIMIT 50",
        )?;
        let mut rows = stmt.query(params![pattern])?;
        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            let json: String = row.get(0)?;
            let mut node: Node =
                serde_json::from_str(&json).context("deserialize search result")?;
            node.fix_node_type();
            if let Node::Function(f) = &mut node {
                if let Some(v) = row.get::<_, Option<i32>>(1)? {
                    f.has_incoming_calls = v != 0;
                }
                if let Some(v) = row.get::<_, Option<i32>>(2)? {
                    f.is_entry_point_candidate = v != 0;
                }
            }
            results.push(node);
        }
        Ok(results)
    }

    /// Get all nodes.
    pub fn get_all_nodes(&self) -> Result<Vec<Node>> {
        let mut stmt = self.conn.prepare(
            "SELECT extra_json, has_incoming_calls, is_entry_point_candidate
             FROM nodes ORDER BY name",
        )?;
        let mut rows = stmt.query([])?;
        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            let json: String = row.get(0)?;
            let mut node: Node =
                serde_json::from_str(&json).context("deserialize all_nodes result")?;
            node.fix_node_type();
            if let Node::Function(f) = &mut node {
                if let Some(v) = row.get::<_, Option<i32>>(1)? {
                    f.has_incoming_calls = v != 0;
                }
                if let Some(v) = row.get::<_, Option<i32>>(2)? {
                    f.is_entry_point_candidate = v != 0;
                }
            }
            results.push(node);
        }
        Ok(results)
    }

    pub fn get_all_edges(&self) -> Result<Vec<Edge>> {
        let mut stmt = self.conn.prepare("SELECT * FROM edges ORDER BY from_id")?;
        let mut rows = stmt.query([])?;
        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            let edge = edge_from_row(row).context("edge row deserialization")?;
            results.push(edge);
        }
        Ok(results)
    }

    /// Count of function nodes.
    pub fn function_count(&self) -> Result<u64> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE type = 'function'",
                [],
                |row| row.get(0),
            )?;
        Ok(count as u64)
    }

    /// Get distinct languages present in the graph.
    pub fn get_distinct_languages(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT language FROM nodes WHERE language IS NOT NULL AND language != ''",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut langs = Vec::new();
        for row in rows {
            langs.push(row?);
        }
        langs.sort();
        Ok(langs)
    }

    /// Count how many function nodes have a non-null ai_summary.
    pub fn annotated_function_count(&self) -> Result<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE type = 'function' AND ai_summary IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    /// Return all flaws, optionally filtered by severity.
    /// Each result contains the FlawFlag + the primary node's name and file_path.
    pub fn query_flaws(
        &self,
        severity_filter: Option<&str>,
    ) -> Result<Vec<(FlawFlag, /* node_name */ String, /* file_path */ Option<String>, /* primary_node_id */ String)>> {
        let sql = match severity_filter {
            Some(_) => {
                "SELECT f.id, f.kind, f.severity, f.description, f.confidence, f.basis,
                        f.primary_node_id, f.related_node_ids_json,
                        n.name, n.file_path
                 FROM flaws f JOIN nodes n ON f.primary_node_id = n.id
                 WHERE f.severity = ?1
                 ORDER BY f.severity, f.id"
            }
            None => {
                "SELECT f.id, f.kind, f.severity, f.description, f.confidence, f.basis,
                        f.primary_node_id, f.related_node_ids_json,
                        n.name, n.file_path
                 FROM flaws f JOIN nodes n ON f.primary_node_id = n.id
                 ORDER BY f.severity, f.id"
            }
        };

        let mut stmt = self.conn.prepare(sql)?;
        let mut rows = if let Some(sev) = severity_filter {
            stmt.query(params![sev])?
        } else {
            stmt.query([])?
        };

        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let kind: String = row.get(1)?;
            let severity: String = row.get(2)?;
            let description: String = row.get(3)?;
            let confidence: f64 = row.get(4)?;
            let basis: String = row.get(5)?;
            let related_json: Option<String> = row.get(7)?;
            let node_name: String = row.get(8)?;
            let file_path: Option<String> = row.get(9)?;
            let primary_node_id: String = row.get(6)?;

            let flaw_kind = match kind.as_str() {
                "dead_code" => super::model::FlawKind::DeadCode,
                "circular_dependency" => super::model::FlawKind::CircularDependency,
                "structural_smell" => super::model::FlawKind::StructuralSmell,
                "suspected_bug" => super::model::FlawKind::SuspectedBug,
                "missing_error_handling" => super::model::FlawKind::MissingErrorHandling,
                "resource_leak_pattern" => super::model::FlawKind::ResourceLeakPattern,
                _ => super::model::FlawKind::StructuralSmell,
            };
            let flaw_severity = match severity.as_str() {
                "info" => super::model::FlawSeverity::Info,
                "warning" => super::model::FlawSeverity::Warning,
                "high" => super::model::FlawSeverity::High,
                _ => super::model::FlawSeverity::Warning,
            };
            let flaw_basis = match basis.as_str() {
                "structural" => super::model::FlawBasis::Structural,
                "ai" => super::model::FlawBasis::Ai,
                "structural+ai" => super::model::FlawBasis::StructuralPlusAi,
                _ => super::model::FlawBasis::Structural,
            };
            let related = related_json
                .and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok());

            results.push((
                FlawFlag {
                    id,
                    kind: flaw_kind,
                    severity: flaw_severity,
                    description,
                    confidence,
                    basis: flaw_basis,
                    related_node_ids: related,
                },
                node_name,
                file_path,
                primary_node_id,
            ));
        }
        Ok(results)
    }

    /// Return flaws for a specific node.
    pub fn get_flaws_for_node(&self, node_id: &str) -> Result<Vec<FlawFlag>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, severity, description, confidence, basis, primary_node_id, related_node_ids_json
             FROM flaws WHERE primary_node_id = ?1 ORDER BY severity",
        )?;
        let mut rows = stmt.query(params![node_id])?;
        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let kind: String = row.get(1)?;
            let severity: String = row.get(2)?;
            let description: String = row.get(3)?;
            let confidence: f64 = row.get(4)?;
            let basis: String = row.get(5)?;
            let related_json: Option<String> = row.get(7)?;

            let flaw_kind = match kind.as_str() {
                "dead_code" => super::model::FlawKind::DeadCode,
                "circular_dependency" => super::model::FlawKind::CircularDependency,
                "structural_smell" => super::model::FlawKind::StructuralSmell,
                "suspected_bug" => super::model::FlawKind::SuspectedBug,
                "missing_error_handling" => super::model::FlawKind::MissingErrorHandling,
                "resource_leak_pattern" => super::model::FlawKind::ResourceLeakPattern,
                _ => super::model::FlawKind::StructuralSmell,
            };
            let flaw_severity = match severity.as_str() {
                "info" => super::model::FlawSeverity::Info,
                "warning" => super::model::FlawSeverity::Warning,
                "high" => super::model::FlawSeverity::High,
                _ => super::model::FlawSeverity::Warning,
            };
            let flaw_basis = match basis.as_str() {
                "structural" => super::model::FlawBasis::Structural,
                "ai" => super::model::FlawBasis::Ai,
                "structural+ai" => super::model::FlawBasis::StructuralPlusAi,
                _ => super::model::FlawBasis::Structural,
            };
            let related = related_json
                .and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok());

            results.push(FlawFlag {
                id,
                kind: flaw_kind,
                severity: flaw_severity,
                description,
                confidence,
                basis: flaw_basis,
                related_node_ids: related,
            });
        }
        Ok(results)
    }

    /// Count of flaws, optionally by severity.
    pub fn flaw_count(&self, severity: Option<&str>) -> Result<u64> {
        let sql = match severity {
            Some(_) => "SELECT COUNT(*) FROM flaws WHERE severity = ?1",
            None => "SELECT COUNT(*) FROM flaws",
        };
        let mut stmt = self.conn.prepare(sql)?;
        let count: i64 = if let Some(sev) = severity {
            stmt.query_row(params![sev], |row| row.get(0))?
        } else {
            stmt.query_row([], |row| row.get(0))?
        };
        Ok(count as u64)
    }

    /// Update has_incoming_calls for all function nodes based on edge data.
    /// Called after a full graph build to set this structural fact correctly.
    pub fn recompute_incoming_calls(&self) -> Result<()> {
        // Reset all to 0
        self.conn.execute(
            "UPDATE nodes SET has_incoming_calls = 0 WHERE type = 'function'",
            [],
        )?;
        // Set 1 for those that have at least one incoming 'calls' edge
        self.conn.execute(
            "UPDATE nodes SET has_incoming_calls = 1
             WHERE type = 'function'
               AND id IN (SELECT DISTINCT to_id FROM edges WHERE type = 'calls')",
            [],
        )?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Row deserializer helpers
// ---------------------------------------------------------------------------

fn edge_from_row(row: &rusqlite::Row) -> Result<Edge> {
    use super::model::{EdgeConfidence, EdgeType};

    let id: String = row.get(0)?;
    let from_id: String = row.get(1)?;
    let to_id: String = row.get(2)?;
    let type_str: String = row.get(3)?;
    let confidence_str: String = row.get(4)?;
    let order_hint: Option<i32> = row.get(5)?;
    let condition: Option<String> = row.get(6)?;

    let edge_type = match type_str.as_str() {
        "calls" => EdgeType::Calls,
        "includes" => EdgeType::Includes,
        "defines" => EdgeType::Defines,
        "references" => EdgeType::References,
        "links_into" => EdgeType::LinksInto,
        "member_of" => EdgeType::MemberOf,
        other => anyhow::bail!("unknown edge type: {other}"),
    };
    let confidence = match confidence_str.as_str() {
        "exact" => EdgeConfidence::Exact,
        "probable" => EdgeConfidence::Probable,
        "dynamic_unresolved" => EdgeConfidence::DynamicUnresolved,
        other => anyhow::bail!("unknown confidence: {other}"),
    };

    Ok(Edge {
        id,
        from_id,
        to_id,
        edge_type,
        confidence,
        order_hint,
        condition,
    })
}

fn ai_summary_status_str(status: &super::model::AiSummaryStatus) -> &'static str {
    match status {
        super::model::AiSummaryStatus::Pending => "pending",
        super::model::AiSummaryStatus::Ready => "ready",
        super::model::AiSummaryStatus::Unavailable => "unavailable",
    }
}

// ---------------------------------------------------------------------------
// Schema DDL (matches doc 03 §6 exactly)
// ---------------------------------------------------------------------------

const SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS nodes (
  id TEXT PRIMARY KEY,
  type TEXT NOT NULL,
  name TEXT NOT NULL,
  language TEXT,
  file_path TEXT,
  start_line INTEGER,
  end_line INTEGER,
  signature TEXT,
  extra_json TEXT NOT NULL DEFAULT '{}',
  ai_summary TEXT,
  ai_summary_status TEXT NOT NULL DEFAULT 'pending',
  ai_model_used TEXT,
  structural_hash TEXT NOT NULL DEFAULT '',
  has_incoming_calls INTEGER,
  is_entry_point_candidate INTEGER,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_nodes_file_path ON nodes(file_path);
CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(type);
CREATE INDEX IF NOT EXISTS idx_nodes_name ON nodes(name);

CREATE TABLE IF NOT EXISTS edges (
  id TEXT PRIMARY KEY,
  from_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  to_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  type TEXT NOT NULL,
  confidence TEXT NOT NULL,
  order_hint INTEGER,
  condition TEXT
);
CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id);
CREATE INDEX IF NOT EXISTS idx_edges_to ON edges(to_id);

CREATE TABLE IF NOT EXISTS flaws (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  severity TEXT NOT NULL,
  description TEXT NOT NULL,
  confidence REAL NOT NULL,
  basis TEXT NOT NULL,
  primary_node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  related_node_ids_json TEXT
);
CREATE INDEX IF NOT EXISTS idx_flaws_node ON flaws(primary_node_id);

CREATE TABLE IF NOT EXISTS files_manifest (
  file_path TEXT PRIMARY KEY,
  content_hash TEXT NOT NULL,
  language TEXT,
  last_parsed_at TEXT NOT NULL,
  parse_status TEXT NOT NULL,
  parse_error TEXT
);
";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::model::*;

    fn make_function_node(id: &str, name: &str, file: &str) -> Node {
        Node::Function(FunctionNode {
            base: BaseNode {
                id: id.to_owned(),
                node_type: NodeType::Function,
                name: name.to_owned(),
                language: Some("rust".to_owned()),
                file_path: Some(file.to_owned()),
                span: Some(Span { start_line: 1, end_line: 5 }),
                ai_summary: None,
                ai_summary_status: AiSummaryStatus::Pending,
                ai_model_used: None,
                structural_hash: "abc123".to_owned(),
                flaws: vec![],
            },
            signature: format!("fn {name}()"),
            is_entry_point_candidate: false,
            has_incoming_calls: false,
            control_flow: None,
        })
    }

    #[test]
    fn upsert_and_get_node() {
        let store = GraphStore::open_in_memory().unwrap();
        let node = make_function_node("id1", "foo", "src/foo.rs");
        store.upsert_node(&node).unwrap();
        let fetched = store.get_node("id1").unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id(), "id1");
    }

    #[test]
    fn node_count() {
        let store = GraphStore::open_in_memory().unwrap();
        store.upsert_node(&make_function_node("a", "a", "f.rs")).unwrap();
        store.upsert_node(&make_function_node("b", "b", "f.rs")).unwrap();
        assert_eq!(store.node_count().unwrap(), 2);
    }

    #[test]
    fn upsert_edge_and_count() {
        let store = GraphStore::open_in_memory().unwrap();
        store.upsert_node(&make_function_node("n1", "caller", "f.rs")).unwrap();
        store.upsert_node(&make_function_node("n2", "callee", "f.rs")).unwrap();

        let edge = Edge {
            id: "e1".to_owned(),
            from_id: "n1".to_owned(),
            to_id: "n2".to_owned(),
            edge_type: EdgeType::Calls,
            confidence: EdgeConfidence::Exact,
            order_hint: Some(0),
            condition: None,
        };
        store.upsert_edge(&edge).unwrap();
        assert_eq!(store.edge_count().unwrap(), 1);
    }

    #[test]
    fn recompute_incoming_calls() {
        let store = GraphStore::open_in_memory().unwrap();
        store.upsert_node(&make_function_node("n1", "caller", "f.rs")).unwrap();
        store.upsert_node(&make_function_node("n2", "callee", "f.rs")).unwrap();

        let edge = Edge {
            id: "e1".to_owned(),
            from_id: "n1".to_owned(),
            to_id: "n2".to_owned(),
            edge_type: EdgeType::Calls,
            confidence: EdgeConfidence::Exact,
            order_hint: Some(0),
            condition: None,
        };
        store.upsert_edge(&edge).unwrap();
        store.recompute_incoming_calls().unwrap();

        // n2 should now have has_incoming_calls = true
        let n2 = store.get_node("n2").unwrap().unwrap();
        if let Node::Function(f) = n2 {
            assert!(f.has_incoming_calls);
        } else {
            panic!("expected function node");
        }
    }

    #[test]
    fn delete_nodes_for_file_cascades() {
        let store = GraphStore::open_in_memory().unwrap();
        store.upsert_node(&make_function_node("n1", "a", "a.rs")).unwrap();
        store.upsert_node(&make_function_node("n2", "b", "b.rs")).unwrap();
        store.delete_nodes_for_file("a.rs").unwrap();
        assert_eq!(store.node_count().unwrap(), 1);
    }
}
