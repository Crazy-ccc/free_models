# /// script
# requires-python = ">=3.9"
# dependencies = [
#     "pymysql",
#     "cryptography",
# ]
# ///
import argparse
import contextlib
import sqlite3
import sys
from datetime import date, datetime
from pathlib import Path
from urllib.parse import unquote, urlparse

try:
    import pymysql
except ImportError:
    print("missing dependency: pip install pymysql cryptography", file=sys.stderr)
    sys.exit(1)

BATCH_SIZE = 5000
SCRIPT_DIR = Path(__file__).resolve().parent
SCHEMA_PATH = SCRIPT_DIR / "free_models_server" / "migrations" / "001_sqlite_schema.sql"

COPY_SPECS = [
    ("provider_config", ("id", "name", "base_url", "created_time", "last_updated")),
    ("model_config", ("id", "name", "priority", "is_active", "context_length", "timeout", "created_time", "last_updated")),
    ("api_key", ("id", "key_value", "name", "is_active", "created_time", "last_updated")),
    ("admin_key", ("id", "name", "public_key", "fingerprint", "is_active", "created_time", "last_updated")),
    ("provider_credential", ("id", "provider_id", "name", "api_key", "account", "encrypted_password", "priority", "quota_exhausted", "is_active", "created_time", "last_updated")),
    ("provider_model_map", ("id", "model_id", "provider_id", "provider_model_id", "is_active", "priority", "context_length", "protocols", "status", "timeout", "created_time", "last_updated")),
    ("usage_log_daily", ("id", "stat_date", "api_key_id", "api_key_name", "provider_config_id", "provider_credential_id", "provider_name", "model_config_id", "model_name", "requests", "prompt_tokens", "completion_tokens", "total_tokens", "cache_hit_tokens", "cache_miss_tokens", "avg_duration_ms", "min_duration_ms", "max_duration_ms", "created_time")),
]

RAW_LOG_SPEC = ("usage_log", ("id", "api_key_id", "api_key_name", "model_config_id", "provider_config_id", "provider_credential_id", "model_name", "provider_name", "protocol", "status", "error_message", "prompt_tokens", "completion_tokens", "total_tokens", "cache_hit_tokens", "cache_miss_tokens", "duration_ms", "is_stream", "request_timestamp"))


def parse_mysql_url(url):
    parsed = urlparse(url)
    if parsed.scheme != "mysql":
        raise ValueError(f"unsupported scheme: {parsed.scheme} (expected mysql://user:pass@127.0.0.1:3306/free_models)")
    return {
        "host": parsed.hostname or "127.0.0.1",
        "port": parsed.port or 3306,
        "user": unquote(parsed.username or ""),
        "password": unquote(parsed.password or ""),
        "database": parsed.path.lstrip("/"),
        "charset": "utf8mb4",
    }


def create_schema(sqlite_conn):
    ddl = SCHEMA_PATH.read_text(encoding="utf-8").replace("\r\n", "\n").replace("\r", "\n")
    statements = []
    buf = []
    for line in ddl.split("\n"):
        stripped = line.strip()
        if stripped.startswith("-- >>>"):
            if buf:
                statements.append("\n".join(buf).strip())
                buf = []
            continue
        if stripped.startswith("--"):
            continue
        buf.append(line)
    if buf:
        stmt = "\n".join(buf).strip()
        if stmt:
            statements.append(stmt)
    for sql in statements:
        sqlite_conn.execute(sql)


def copy_table(mysql_conn, sqlite_conn, table, columns):
    col_list = ", ".join(columns)
    placeholders = ", ".join("?" * len(columns))
    insert_sql = f"INSERT INTO {table} ({col_list}) VALUES ({placeholders})"
    select_sql = f"SELECT {col_list} FROM {table} WHERE id > %s ORDER BY id LIMIT %s"
    copied = 0
    cursor_id = -1
    with mysql_conn.cursor() as cur:
        while True:
            cur.execute(select_sql, (cursor_id, BATCH_SIZE))
            rows = cur.fetchall()
            fetched = len(rows)
            if fetched == 0:
                break
            sqlite_conn.executemany(insert_sql, rows)
            copied += fetched
            cursor_id = rows[-1][0]
            if fetched < BATCH_SIZE:
                break
    return copied


def count_rows(conn, table):
    return conn.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]


def main():
    parser = argparse.ArgumentParser(description="Migrate free_models data from MySQL into a single SQLite file")
    parser.add_argument("--mysql-url", required=True, help="e.g. mysql://user:pass@127.0.0.1:3306/free_models")
    parser.add_argument("--output", default=str(SCRIPT_DIR / "free_models.db"), help="output sqlite path (default: free_models.db next to this script)")
    parser.add_argument("--include-raw-logs", action="store_true", help="also copy usage_log raw logs (skipped by default)")
    args = parser.parse_args()

    mysql = None
    sqlite_conn = None
    try:
        mysql = pymysql.connect(**parse_mysql_url(args.mysql_url), autocommit=True)
        print("[1/5] connected to source mysql")

        output = Path(args.output).resolve()
        for suffix in ("", "-wal", "-shm"):
            stale = Path(str(output) + suffix)
            if stale.exists():
                stale.unlink()
                print(f"removed existing file: {stale}")

        sqlite_conn = sqlite3.connect(output)
        sqlite_conn.isolation_level = None
        sqlite_conn.execute("PRAGMA journal_mode=WAL")
        sqlite_conn.execute("PRAGMA busy_timeout=5000")
        sqlite_conn.execute("PRAGMA foreign_keys=ON")
        sqlite3.register_adapter(datetime, lambda d: d.isoformat(sep=" ", timespec="microseconds" if d.microsecond else "seconds"))
        sqlite3.register_adapter(date, lambda d: d.isoformat())
        print(f"[2/5] sqlite file created: {output}")

        create_schema(sqlite_conn)
        print("[3/5] schema applied from migrations/001_sqlite_schema.sql")

        print("[4/5] copying data...")
        specs = list(COPY_SPECS)
        if args.include_raw_logs:
            specs.append(RAW_LOG_SPEC)
        else:
            print("skipping usage_log (raw logs); pass --include-raw-logs to copy them")

        results = []
        for table, columns in specs:
            sqlite_conn.execute("BEGIN")
            try:
                copied = copy_table(mysql, sqlite_conn, table, columns)
                sqlite_conn.execute("COMMIT")
            except Exception:
                with contextlib.suppress(sqlite3.Error):
                    sqlite_conn.execute("ROLLBACK")
                raise
            results.append((table, copied, count_rows(sqlite_conn, table)))

        print("[5/5] verification (copied / sqlite-count):")
        all_match = True
        for table, copied, target in results:
            ok = copied == target
            all_match = all_match and ok
            print(f"  {table:<22} copied={copied:<8} sqlite={target} {'OK' if ok else 'MISMATCH'}")
        print(f"done. point DATABASE_URL at: sqlite://{output}?mode=rw")
        if not all_match:
            print("row counts mismatched, inspect the report above", file=sys.stderr)
            sys.exit(1)
    finally:
        if mysql is not None:
            with contextlib.suppress(Exception):
                mysql.close()
        if sqlite_conn is not None:
            with contextlib.suppress(sqlite3.Error):
                sqlite_conn.close()


if __name__ == "__main__":
    main()
