# OwnMon API Documentation

**Base URL:** `http://127.0.0.1:13234`

---

## ⚠️ Performance & Configuration

| Setting | Default | Configurable | Notes |
|---------|---------|--------------|-------|
| Default limit | 500 | ✓ DB | Prevents memory issues |
| Maximum limit | 2,000 | ✓ DB | Hard cap on results |
| Min session duration | 3 seconds | ✓ DB | Sessions <3s discarded |
| AFK threshold | 5 minutes | ✓ DB | Idle detection |
| Poll interval | 100ms | ✓ DB | Window check frequency |

> **Warning:** Querying large date ranges without limits may cause slow responses. Always use pagination.

> **Session Filtering:** Sessions shorter than `min_session_duration_secs` (default: 3s) are automatically discarded to reduce noise from window switches.

> **AFK Tracking:** Sessions are **automatically split** when >`afk_threshold_secs` (default: 300s) of inactivity is detected. Idle sessions have `is_idle=true`. Calculate idle time: `idle_secs = is_idle ? duration_secs : 0`

---

## Health Check

### `GET /health`
Server health check.

**Response:**
```json
{
  "status": "ok",
  "version": "0.1.0"
}
```

---

## Sessions API

### `GET /api/sessions`
Query window focus sessions with flexible filtering.

**Query Parameters:**
| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `date` | string | *today* | Filter by date (YYYY-MM-DD) |
| `from` | string | - | Start time (ISO 8601) |
| `to` | string | - | End time (ISO 8601) |
| `app` | string | - | Process name filter (`*` wildcards supported) |
| `category` | integer | - | Filter by category ID |
| `limit` | integer | 500 | Max results (max: 2000) |
| `offset` | integer | 0 | Pagination offset |
| `order` | string | "desc" | Sort order ("asc" or "desc") |

**Response:**
```json
{
  "sessions": [
    {
      "id": 123,
      "process_name": "chrome.exe",
      "window_title": "Google - Chrome",
      "start_time": "2025-12-13T15:30:00+00:00",
      "end_time": "2025-12-13T15:45:00+00:00",
      "keystrokes": 50,
      "clicks": 30,
      "scrolls": 10,
      "is_idle": false,
      "duration_secs": 900,
      "category": {
        "id": 5,
        "name": "Browser",
        "color": "#F59E0B",
        "icon": "🌐"
      }
    }
  ],
  "total": 150,
  "limit": 500,
  "offset": 0
}
```

> **Note:** 
> - Sessions shorter than `min_session_duration_secs` (default: 3s) are **not saved** to reduce noise
> - Sessions are **split** when idle >`afk_threshold_secs` (default: 300s)
> - `is_idle=true` indicates an idle/AFK session with zero input activity
> - Calculate idle time: `idle_secs = is_idle ? duration_secs : 0`

---

## Media API

### `GET /api/media`
Query media playback history with flexible filtering.

**Query Parameters:**
| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `date` | string | *today* | Filter by date (YYYY-MM-DD) |
| `from` | string | - | Start time (ISO 8601) |
| `to` | string | - | End time (ISO 8601) |
| `artist` | string | - | Artist filter (`*` wildcards supported) |
| `source_app` | string | - | Source app filter (`*` wildcards) |
| `limit` | integer | 500 | Max results (max: 2000) |
| `offset` | integer | 0 | Pagination offset |
| `order` | string | "desc" | Sort order ("asc" or "desc") |

**Response:**
```json
{
  "current": {
    "title": "Song Name",
    "artist": "Artist Name",
    "album": "Album Name",
    "source_app": "Spotify.exe",
    "start_time": "2025-12-13T15:30:00+00:00",
    "duration_secs": 180,
    "is_playing": true
  },
  "history": [
    {
      "id": 45,
      "title": "Previous Song",
      "artist": "Artist",
      "album": "Album",
      "source_app": "Spotify.exe",
      "start_time": "2025-12-13T15:25:00+00:00",
      "end_time": "2025-12-13T15:28:00+00:00",
      "duration_secs": 180
    }
  ],
  "total": 25,
  "limit": 500,
  "offset": 0
}
```

---

## Statistics API

### `GET /api/stats`
Today's summary statistics (from memory).

**Response:**
```json
{
  "sessions": 45,
  "unique_apps": 12,
  "keystrokes": 5420,
  "clicks": 1230,
  "focus_time_secs": 14400,
  "media_time_secs": 3600
}
```

---

### `GET /api/stats/daily`
Aggregated stats for a specific date (from database).

**Query Parameters:**
| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `date` | string | *today* | Date (YYYY-MM-DD) |

**Response:**
```json
{
  "date": "2025-12-13",
  "keystrokes": 5420,
  "clicks": 1230,
  "focus_secs": 14400
}
```

---

### `GET /api/stats/hourly`
Hourly breakdown for charts.

**Query Parameters:**
| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `date` | string | *today* | Date (YYYY-MM-DD) |

**Response:**
```json
[
  {"hour": 9, "keystrokes": 500, "clicks": 120, "sessions": 5, "focus_secs": 3600},
  {"hour": 10, "keystrokes": 800, "clicks": 200, "sessions": 8, "focus_secs": 3500}
]
```

---

### `GET /api/stats/timeline`
Daily totals for trend charts.

**Query Parameters:**
| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `days` | integer | 7 | Number of days to include |

**Response:**
```json
[
  {"date": "2025-12-12", "keystrokes": 4500, "clicks": 1100, "sessions": 40, "focus_secs": 28800},
  {"date": "2025-12-13", "keystrokes": 5420, "clicks": 1230, "sessions": 45, "focus_secs": 14400}
]
```

---

## Apps API

### `GET /api/apps`
Top applications ranked by focus time (from memory).

**Response:**
```json
[
  {
    "process_name": "chrome.exe",
    "focus_time_secs": 7200,
    "keystrokes": 3000,
    "clicks": 800,
    "session_count": 25
  }
]
```

---

### `GET /api/apps/:name/category`
Get category for a specific app.

**Path Parameters:**
| Param | Type | Description |
|-------|------|-------------|
| `name` | string | Process name (e.g., `chrome.exe`) |

**Response:**
```json
{
  "id": 5,
  "name": "Browser",
  "color": "#F59E0B",
  "icon": "🌐"
}
```

---

## Configuration API

### `GET /api/config`
Get all configuration settings from the database.

**Response:**
```json
{
  "settings": [
    {
      "key": "min_session_duration_secs",
      "value": "3",
      "description": "Minimum session duration to save (seconds)"
    },
    {
      "key": "afk_threshold_secs",
      "value": "300",
      "description": "Idle/AFK detection threshold (seconds)"
    },
    {
      "key": "poll_interval_ms",
      "value": "100",
      "description": "Window polling interval (milliseconds)"
    },
    {
      "key": "track_title_changes",
      "value": "false",
      "description": "Track title changes within same process"
    },
    {
      "key": "max_sessions",
      "value": "1000",
      "description": "Maximum sessions to keep in memory"
    },
    {
      "key": "prune_interval_secs",
      "value": "3600",
      "description": "How often to prune old sessions (seconds)"
    }
  ]
}
```

> **Note:** Configuration values are stored in the database and can be modified directly via SQL. Changes take effect on next read.

---

## Categories API

### `GET /api/categories`
List all categories.

**Response:**
```json
[
  {"id": 1, "name": "Other", "color": "#9CA3AF", "icon": "📁"},
  {"id": 2, "name": "Work", "color": "#3B82F6", "icon": "💼"},
  {"id": 3, "name": "Entertainment", "color": "#EF4444", "icon": "🎮"},
  {"id": 4, "name": "Communication", "color": "#10B981", "icon": "💬"},
  {"id": 5, "name": "Browser", "color": "#F59E0B", "icon": "🌐"},
  {"id": 6, "name": "System", "color": "#6B7280", "icon": "⚙️"}
]
```

---

## WebSocket

### `WS /ws`
Real-time updates via WebSocket.

**On Connection:**
Server immediately sends `initial_state` with current activity:
```json
{
  "type": "initial_state",
  "data": {
    "session": {
      "process_name": "chrome.exe",
      "window_title": "Google - Chrome",
      "start_time": "2025-12-13T15:30:00+00:00"
    },
    "media": {
      "title": "Song Name",
      "artist": "Artist",
      "album": "Album",
      "is_playing": true,
      "start_time": "2025-12-13T15:25:00+00:00"
    },
    "stats": {
      "sessions": 45,
      "keystrokes": 5420,
      "clicks": 1230,
      "focus_time_secs": 14400
    }
  },
  "timestamp": "2025-12-13T15:45:00+00:00"
}
```

**Update Messages:**
```json
{
  "type": "session_change",
  "data": {
    "process_name": "chrome.exe",
    "window_title": "Google - Chrome"
  },
  "timestamp": "2025-12-13T15:45:00+00:00"
}
```

**Event Types:**
| Type | Description |
|------|-------------|
| `initial_state` | Sent on connection with current state |
| `session_change` | Window focus changed |
| `media_update` | Media playback changed |

---

## Error Handling

All endpoints return empty arrays `[]` or `null` on errors. HTTP status is always 200.

---

## Date/Time Formats

| Format | Example | Usage |
|--------|---------|-------|
| Date | `2025-12-13` | `date` parameter |
| ISO 8601 | `2025-12-13T15:30:00+07:00` | `from`, `to` parameters |
| RFC 3339 | `2025-12-13T08:30:00+00:00` | Response timestamps |
