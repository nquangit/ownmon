//! SQLite database for persistent activity storage.
//!
//! This module provides crash-safe persistence for activity data.
//! Data is saved periodically and on session changes to minimize loss.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result as SqlResult};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Database wrapper with thread-safe connection.
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Opens or creates the database at the default location.
    ///
    /// Creates %APPDATA%/ownmon/activity.db if it doesn't exist.
    pub fn open() -> SqlResult<Self> {
        let db_path = Self::get_db_path();

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        tracing::info!(path = ?db_path, "Opening database");

        let conn = Connection::open(&db_path)?;

        // Enable WAL mode for better crash safety
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        db.init_schema()?;

        Ok(db)
    }

    /// Opens an in-memory database (for testing).
    #[cfg(test)]
    pub fn open_in_memory() -> SqlResult<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init_schema()?;
        Ok(db)
    }

    /// Returns the default database path.
    fn get_db_path() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ownmon")
            .join("activity.db")
    }

    /// Initializes the database schema.
    fn init_schema(&self) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            r#"
            -- Window sessions
            CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                process_name TEXT NOT NULL,
                window_title TEXT,
                start_time TEXT NOT NULL,
                end_time TEXT,
                keystrokes INTEGER DEFAULT 0,
                clicks INTEGER DEFAULT 0,
                scrolls INTEGER DEFAULT 0,
                is_idle BOOLEAN DEFAULT 0
            );

            -- Media playback
            CREATE TABLE IF NOT EXISTS media (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                artist TEXT,
                album TEXT,
                source_app TEXT,
                start_time TEXT NOT NULL,
                end_time TEXT,
                duration_secs INTEGER DEFAULT 0
            );

            -- Blacklist for apps to ignore
            -- Patterns support wildcards: * matches any chars, ? matches single char
            CREATE TABLE IF NOT EXISTS blacklist (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                pattern TEXT NOT NULL UNIQUE,
                description TEXT,
                created_at TEXT NOT NULL
            );

            -- App categories for grouping
            CREATE TABLE IF NOT EXISTS categories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                color TEXT NOT NULL,
                icon TEXT,
                created_at TEXT NOT NULL
            );

            -- App to category mapping (patterns support wildcards)
            CREATE TABLE IF NOT EXISTS app_categories (
                process_pattern TEXT PRIMARY KEY,
                category_id INTEGER REFERENCES categories(id)
            );

            -- Configuration settings
            CREATE TABLE IF NOT EXISTS config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                description TEXT,
                updated_at TEXT NOT NULL
            );

            -- Indexes for date queries
            CREATE INDEX IF NOT EXISTS idx_sessions_start ON sessions(start_time);
            CREATE INDEX IF NOT EXISTS idx_media_start ON media(start_time);
            "#,
        )?;

        // Insert default blacklist entries if table is empty
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM blacklist", [], |r| r.get(0))?;
        if count == 0 {
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO blacklist (pattern, description, created_at) VALUES (?1, ?2, ?3)",
                params!["ownmon.exe", "Self (monitoring app)", &now],
            )?;
            tracing::info!("Added default blacklist entry: ownmon.exe");
        }

        // Insert default categories if empty
        let cat_count: i64 = conn.query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))?;
        if cat_count == 0 {
            let now = Utc::now().to_rfc3339();

            // Insert preset categories
            let presets = [
                ("Other", "#9CA3AF", "📁"), // Default for uncategorized (ID=1)
                ("Work", "#3B82F6", "💼"),
                ("Entertainment", "#EF4444", "🎮"),
                ("Communication", "#10B981", "💬"),
                ("Browser", "#F59E0B", "🌐"),
                ("System", "#6B7280", "⚙️"),
            ];

            for (name, color, icon) in presets {
                conn.execute(
                    "INSERT INTO categories (name, color, icon, created_at) VALUES (?1, ?2, ?3, ?4)",
                    params![name, color, icon, &now],
                )?;
            }

            // Insert default app mappings
            let app_mappings = [
                // Work
                ("code.exe", 2),
                ("Code.exe", 2),
                ("devenv.exe", 2),
                ("Antigravity.exe", 2),
                ("rider64.exe", 2),
                ("idea64.exe", 2),
                ("pycharm64.exe", 2),
                // Entertainment
                ("Spotify.exe", 3),
                ("vlc.exe", 3),
                ("Netflix*", 3),
                ("*YouTube*", 3),
                // Communication
                ("Slack.exe", 4),
                ("Discord.exe", 4),
                ("Teams.exe", 4),
                ("Telegram.exe", 4),
                ("WhatsApp.exe", 4),
                ("Postman.exe", 4),
                // Browser
                ("chrome.exe", 5),
                ("firefox.exe", 5),
                ("msedge.exe", 5),
                ("brave.exe", 5),
                // System
                ("Explorer.EXE", 6),
                ("SearchHost.exe", 6),
                ("Taskmgr.exe", 6),
            ];

            for (pattern, cat_id) in app_mappings {
                conn.execute(
                    "INSERT OR IGNORE INTO app_categories (process_pattern, category_id) VALUES (?1, ?2)",
                    params![pattern, cat_id],
                )?;
            }

            tracing::info!(
                "Added {} preset categories and {} app mappings",
                presets.len(),
                app_mappings.len()
            );
        }

        // Seed default config if empty
        let config_count: i64 = conn.query_row("SELECT COUNT(*) FROM config", [], |r| r.get(0))?;
        if config_count == 0 {
            let now = Utc::now().to_rfc3339();
            let defaults = [
                (
                    "min_session_duration_secs",
                    "10",
                    "Minimum session duration to save (seconds)",
                ),
                (
                    "afk_threshold_secs",
                    "300",
                    "Idle/AFK detection threshold (seconds)",
                ),
                (
                    "poll_interval_ms",
                    "100",
                    "Window polling interval (milliseconds)",
                ),
                (
                    "track_title_changes",
                    "false",
                    "Track title changes within same process",
                ),
                ("max_sessions", "1000", "Maximum sessions to keep in memory"),
                (
                    "prune_interval_secs",
                    "3600",
                    "How often to prune old sessions (seconds)",
                ),
            ];

            for (key, value, description) in defaults {
                conn.execute(
                    "INSERT INTO config (key, value, description, updated_at) VALUES (?1, ?2, ?3, ?4)",
                    params![key, value, description, &now],
                )?;
            }

            tracing::info!("Added {} default config settings", defaults.len());
        }

        tracing::debug!("Database schema initialized");
        Ok(())
    }

    /// Saves a completed window session.
    pub fn save_session(
        &self,
        process_name: &str,
        window_title: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        keystrokes: u64,
        clicks: u64,
        scrolls: u64,
        is_idle: bool,
    ) -> SqlResult<i64> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "INSERT INTO sessions (process_name, window_title, start_time, end_time, keystrokes, clicks, scrolls, is_idle)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                process_name,
                window_title,
                start_time.to_rfc3339(),
                end_time.to_rfc3339(),
                keystrokes as i64,
                clicks as i64,
                scrolls as i64,
                is_idle,
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Saves a completed media session.
    pub fn save_media(
        &self,
        title: &str,
        artist: &str,
        album: &str,
        source_app: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> SqlResult<i64> {
        let duration_secs = (end_time - start_time).num_seconds().max(0);
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "INSERT INTO media (title, artist, album, source_app, start_time, end_time, duration_secs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                title,
                artist,
                album,
                source_app,
                start_time.to_rfc3339(),
                end_time.to_rfc3339(),
                duration_secs,
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Queries media with flexible filtering.
    /// Returns (media_records, total_count).
    pub fn query_media_flexible(
        &self,
        date: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        artist: Option<&str>,
        source_app: Option<&str>,
        limit: usize,
        offset: usize,
        order_desc: bool,
    ) -> SqlResult<(Vec<MediaRecord>, i64)> {
        let conn = self.conn.lock().unwrap();

        let mut conditions = vec!["end_time IS NOT NULL".to_string()];

        if let Some(d) = date {
            conditions.push(format!("start_time LIKE '{}%'", d));
        }
        if let Some(f) = from {
            conditions.push(format!("start_time >= '{}'", f));
        }
        if let Some(t) = to {
            conditions.push(format!("start_time <= '{}'", t));
        }
        if let Some(a) = artist {
            if a.contains('*') {
                let pattern = a.replace('*', "%");
                conditions.push(format!("artist LIKE '{}'", pattern));
            } else {
                conditions.push(format!("artist = '{}'", a));
            }
        }
        if let Some(s) = source_app {
            if s.contains('*') {
                let pattern = s.replace('*', "%");
                conditions.push(format!("source_app LIKE '{}'", pattern));
            } else {
                conditions.push(format!("source_app = '{}'", s));
            }
        }

        let where_clause = conditions.join(" AND ");
        let order_sql = if order_desc { "DESC" } else { "ASC" };

        // Get total count
        let count_sql = format!("SELECT COUNT(*) FROM media WHERE {}", where_clause);
        let total: i64 = conn
            .query_row(&count_sql, [], |row| row.get(0))
            .unwrap_or(0);

        // Get media
        let sql = format!(
            "SELECT id, title, artist, album, source_app, start_time, end_time, duration_secs
             FROM media 
             WHERE {}
             ORDER BY start_time {}
             LIMIT {} OFFSET {}",
            where_clause, order_sql, limit, offset
        );

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            Ok(MediaRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                artist: row.get(2)?,
                album: row.get(3)?,
                source_app: row.get(4)?,
                start_time: row.get(5)?,
                end_time: row.get(6)?,
                duration_secs: row.get(7)?,
            })
        })?;

        let media: Vec<MediaRecord> = rows.filter_map(|r| r.ok()).collect();
        Ok((media, total))
    }

    /// Gets session count for today.
    pub fn get_today_session_count(&self) -> SqlResult<i64> {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let conn = self.conn.lock().unwrap();

        conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE start_time LIKE ?1 || '%'",
            params![today],
            |row| row.get(0),
        )
    }

    /// Gets aggregated stats for a specific date (computed from sessions).
    pub fn get_stats_for_date(&self, date: &str) -> SqlResult<(i64, i64, i64)> {
        let conn = self.conn.lock().unwrap();

        conn.query_row(
            "SELECT 
                COALESCE(SUM(keystrokes), 0),
                COALESCE(SUM(clicks), 0),
                COALESCE(SUM(
                    CAST((julianday(end_time) - julianday(start_time)) * 86400 AS INTEGER)
                ), 0)
             FROM sessions 
             WHERE start_time LIKE ?1 || '%' AND end_time IS NOT NULL",
            params![date],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
    }

    /// Gets hourly breakdown for a specific date (for charts).
    pub fn get_hourly_stats(&self, date: &str) -> SqlResult<Vec<HourlyStats>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT 
                CAST(strftime('%H', start_time) AS INTEGER) as hour,
                COALESCE(SUM(keystrokes), 0) as keystrokes,
                COALESCE(SUM(clicks), 0) as clicks,
                COUNT(*) as sessions,
                COALESCE(SUM(
                    CAST((julianday(end_time) - julianday(start_time)) * 86400 AS INTEGER)
                ), 0) as focus_secs
             FROM sessions 
             WHERE start_time LIKE ?1 || '%' AND end_time IS NOT NULL
             GROUP BY hour
             ORDER BY hour",
        )?;

        let rows = stmt.query_map(params![date], |row| {
            Ok(HourlyStats {
                hour: row.get(0)?,
                keystrokes: row.get(1)?,
                clicks: row.get(2)?,
                sessions: row.get(3)?,
                focus_secs: row.get(4)?,
            })
        })?;

        rows.collect()
    }

    /// Gets daily timeline for the last N days (for trend charts).
    pub fn get_timeline(&self, days: i32) -> SqlResult<Vec<DailyTimeline>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT 
                DATE(start_time) as date,
                COALESCE(SUM(keystrokes), 0) as keystrokes,
                COALESCE(SUM(clicks), 0) as clicks,
                COUNT(*) as sessions,
                COALESCE(SUM(
                    CAST((julianday(end_time) - julianday(start_time)) * 86400 AS INTEGER)
                ), 0) as focus_secs
             FROM sessions 
             WHERE start_time >= date('now', ?1 || ' days') AND end_time IS NOT NULL
             GROUP BY date
             ORDER BY date",
        )?;

        let offset = format!("-{}", days);
        let rows = stmt.query_map(params![offset], |row| {
            Ok(DailyTimeline {
                date: row.get(0)?,
                keystrokes: row.get(1)?,
                clicks: row.get(2)?,
                sessions: row.get(3)?,
                focus_secs: row.get(4)?,
            })
        })?;

        rows.collect()
    }

    // === Category Methods ===

    /// Gets all categories.
    pub fn get_categories(&self) -> SqlResult<Vec<Category>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, color, icon FROM categories ORDER BY id")?;

        let rows = stmt.query_map([], |row| {
            Ok(Category {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                icon: row.get(3)?,
            })
        })?;

        rows.collect()
    }

    /// Gets the category for a process name (returns "Other" category ID=1 if not found).
    pub fn get_category_for_app(&self, process_name: &str) -> SqlResult<Category> {
        let conn = self.conn.lock().unwrap();

        // Try exact match first
        if let Ok(cat) = conn.query_row(
            "SELECT c.id, c.name, c.color, c.icon 
             FROM categories c
             JOIN app_categories ac ON ac.category_id = c.id
             WHERE ac.process_pattern = ?1",
            params![process_name],
            |row| {
                Ok(Category {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    icon: row.get(3)?,
                })
            },
        ) {
            return Ok(cat);
        }

        // Try pattern matching with wildcards
        let patterns: Vec<(String, i64)> = {
            let mut stmt =
                conn.prepare("SELECT process_pattern, category_id FROM app_categories")?;
            let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.filter_map(|r| r.ok()).collect()
        };

        let name_lower = process_name.to_lowercase();
        for (pattern, cat_id) in patterns {
            if pattern_matches(&pattern.to_lowercase(), &name_lower) {
                return conn.query_row(
                    "SELECT id, name, color, icon FROM categories WHERE id = ?1",
                    params![cat_id],
                    |row| {
                        Ok(Category {
                            id: row.get(0)?,
                            name: row.get(1)?,
                            color: row.get(2)?,
                            icon: row.get(3)?,
                        })
                    },
                );
            }
        }

        // Default to "Other" (ID=1)
        conn.query_row(
            "SELECT id, name, color, icon FROM categories WHERE id = 1",
            [],
            |row| {
                Ok(Category {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    icon: row.get(3)?,
                })
            },
        )
    }

    /// Assigns an app to a category.
    pub fn set_app_category(&self, process_pattern: &str, category_id: i64) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO app_categories (process_pattern, category_id) VALUES (?1, ?2)",
            params![process_pattern, category_id],
        )?;
        Ok(())
    }

    // === Config Methods ===

    /// Gets a configuration value by key.
    pub fn get_config(&self, key: &str) -> SqlResult<Option<String>> {
        let conn = self.conn.lock().unwrap();
        match conn.query_row(
            "SELECT value FROM config WHERE key = ?1",
            params![key],
            |row| row.get(0),
        ) {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Sets a configuration value.
    pub fn set_config(&self, key: &str, value: &str) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE config SET value = ?1, updated_at = ?2 WHERE key = ?3",
            params![value, &now, key],
        )?;
        Ok(())
    }

    /// Gets all config settings.
    pub fn get_all_config(&self) -> SqlResult<Vec<(String, String, Option<String>)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key, value, description FROM config ORDER BY key")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        rows.collect()
    }

    /// Gets recent sessions (for reports).
    pub fn get_recent_sessions(&self, limit: usize) -> SqlResult<Vec<SessionRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, process_name, window_title, start_time, end_time, keystrokes, clicks, scrolls, is_idle
             FROM sessions ORDER BY id DESC LIMIT ?1"
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(SessionRecord {
                id: row.get(0)?,
                process_name: row.get(1)?,
                window_title: row.get(2)?,
                start_time: row.get(3)?,
                end_time: row.get(4)?,
                keystrokes: row.get(5)?,
                clicks: row.get(6)?,
                scrolls: row.get(7)?,
                is_idle: row.get(8)?,
            })
        })?;

        rows.collect()
    }

    /// Queries sessions with flexible filtering.
    /// Returns (sessions, total_count).
    pub fn query_sessions_flexible(
        &self,
        date: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        app: Option<&str>,
        limit: usize,
        offset: usize,
        order_desc: bool,
    ) -> SqlResult<(Vec<SessionWithDuration>, i64)> {
        let conn = self.conn.lock().unwrap();

        let mut conditions = vec!["end_time IS NOT NULL".to_string()];

        // Build WHERE clause
        if let Some(d) = date {
            conditions.push(format!("start_time LIKE '{}%'", d));
        }
        if let Some(f) = from {
            conditions.push(format!("start_time >= '{}'", f));
        }
        if let Some(t) = to {
            conditions.push(format!("start_time <= '{}'", t));
        }
        if let Some(a) = app {
            if a.contains('*') {
                let pattern = a.replace('*', "%");
                conditions.push(format!("process_name LIKE '{}'", pattern));
            } else {
                conditions.push(format!("process_name = '{}'", a));
            }
        }

        let where_clause = conditions.join(" AND ");
        let order_sql = if order_desc { "DESC" } else { "ASC" };

        // Get total count
        let count_sql = format!("SELECT COUNT(*) FROM sessions WHERE {}", where_clause);
        let total: i64 = conn
            .query_row(&count_sql, [], |row| row.get(0))
            .unwrap_or(0);

        // Get sessions with duration
        let sql = format!(
            "SELECT id, process_name, window_title, start_time, end_time, keystrokes, clicks, scrolls, is_idle,
                    CAST((julianday(end_time) - julianday(start_time)) * 86400 AS INTEGER) as duration
             FROM sessions 
             WHERE {}
             ORDER BY start_time {}
             LIMIT {} OFFSET {}",
            where_clause, order_sql, limit, offset
        );

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            Ok(SessionWithDuration {
                id: row.get(0)?,
                process_name: row.get(1)?,
                window_title: row.get(2)?,
                start_time: row.get(3)?,
                end_time: row.get(4)?,
                keystrokes: row.get(5)?,
                clicks: row.get(6)?,
                scrolls: row.get(7)?,
                is_idle: row.get(8)?,
                duration_secs: row.get(9)?,
            })
        })?;

        let sessions: Vec<SessionWithDuration> = rows.filter_map(|r| r.ok()).collect();
        Ok((sessions, total))
    }

    // === Blacklist Methods ===

    /// Gets all blacklist patterns.
    pub fn get_blacklist(&self) -> SqlResult<Vec<BlacklistEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, pattern, description, created_at FROM blacklist ORDER BY id")?;

        let rows = stmt.query_map([], |row| {
            Ok(BlacklistEntry {
                id: row.get(0)?,
                pattern: row.get(1)?,
                description: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        rows.collect()
    }

    /// Adds a pattern to the blacklist.
    pub fn add_to_blacklist(&self, pattern: &str, description: Option<&str>) -> SqlResult<i64> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT OR IGNORE INTO blacklist (pattern, description, created_at) VALUES (?1, ?2, ?3)",
            params![pattern, description, now],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Removes a pattern from the blacklist.
    pub fn remove_from_blacklist(&self, pattern: &str) -> SqlResult<bool> {
        let conn = self.conn.lock().unwrap();
        let affected =
            conn.execute("DELETE FROM blacklist WHERE pattern = ?1", params![pattern])?;
        Ok(affected > 0)
    }

    /// Checks if a process name matches any blacklist pattern.
    pub fn is_blacklisted(&self, process_name: &str) -> bool {
        let patterns = match self.get_blacklist() {
            Ok(entries) => entries,
            Err(_) => return false,
        };

        let name_lower = process_name.to_lowercase();

        for entry in patterns {
            if pattern_matches(&entry.pattern.to_lowercase(), &name_lower) {
                return true;
            }
        }

        false
    }
}

/// Matches a pattern with wildcards against a string.
/// * matches zero or more characters
/// ? matches exactly one character
fn pattern_matches(pattern: &str, text: &str) -> bool {
    let p_chars = pattern.chars().peekable();
    let t_chars = text.chars().peekable();

    fn match_helper(
        mut p: std::iter::Peekable<std::str::Chars>,
        mut t: std::iter::Peekable<std::str::Chars>,
    ) -> bool {
        loop {
            match (p.next(), t.peek()) {
                (Some('*'), _) => {
                    // Skip consecutive stars
                    while p.peek() == Some(&'*') {
                        p.next();
                    }
                    // If pattern ends with *, match everything
                    if p.peek().is_none() {
                        return true;
                    }
                    // Try matching rest of pattern at each position
                    loop {
                        if match_helper(p.clone(), t.clone()) {
                            return true;
                        }
                        if t.next().is_none() {
                            return false;
                        }
                    }
                }
                (Some('?'), Some(_)) => {
                    t.next();
                }
                (Some(pc), Some(&tc)) if pc == tc => {
                    t.next();
                }
                (None, None) => return true,
                _ => return false,
            }
        }
    }

    match_helper(p_chars, t_chars)
}

/// A blacklist entry from the database.
#[derive(Debug, Clone)]
pub struct BlacklistEntry {
    pub id: i64,
    pub pattern: String,
    pub description: Option<String>,
    pub created_at: String,
}

/// A session record from the database.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionRecord {
    pub id: i64,
    pub process_name: String,
    pub window_title: Option<String>,
    pub start_time: String,
    pub end_time: Option<String>,
    pub keystrokes: i64,
    pub clicks: i64,
    pub scrolls: i64,
    pub is_idle: bool,
}

/// Hourly stats for chart data.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HourlyStats {
    pub hour: i32,
    pub keystrokes: i64,
    pub clicks: i64,
    pub sessions: i64,
    pub focus_secs: i64,
}

/// Daily timeline entry for trend charts.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DailyTimeline {
    pub date: String,
    pub keystrokes: i64,
    pub clicks: i64,
    pub sessions: i64,
    pub focus_secs: i64,
}

/// App category.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub icon: Option<String>,
}

/// Session record with computed duration.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionWithDuration {
    pub id: i64,
    pub process_name: String,
    pub window_title: Option<String>,
    pub start_time: String,
    pub end_time: Option<String>,
    pub keystrokes: i64,
    pub clicks: i64,
    pub scrolls: i64,
    pub is_idle: bool,
    pub duration_secs: i64,
}

/// Media record from the database.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MediaRecord {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub source_app: String,
    pub start_time: String,
    pub end_time: Option<String>,
    pub duration_secs: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_database() {
        let db = Database::open_in_memory().unwrap();
        assert!(db.get_today_session_count().is_ok());
    }

    #[test]
    fn test_save_and_retrieve_session() {
        let db = Database::open_in_memory().unwrap();

        let start = Utc::now();
        let end = start + chrono::Duration::seconds(60);

        let id = db
            .save_session("test.exe", "Test Window", start, end, 100, 50, 10, false)
            .unwrap();

        assert!(id > 0);

        let sessions = db.get_recent_sessions(10).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].process_name, "test.exe");
        assert_eq!(sessions[0].keystrokes, 100);
    }
}
