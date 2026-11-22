//! Reading SQLite files that NeonDojo did not write (Anki collections,
//! BackgammonDB opening books). Their schema is attacker-controlled: a view,
//! trigger or virtual table can stand in for an expected table and run SQL
//! functions while NeonDojo reads it.
use rusqlite::{Connection, OpenFlags, config::DbConfig};
use std::path::Path;

/// Opens `path` read-only with schema hardening: SQL functions are not
/// trusted inside views, triggers or virtual tables of the file
/// (`TRUSTED_SCHEMA` off), and the defensive mode refuses statements that
/// could corrupt it.
pub(crate) fn open(path: impl AsRef<Path>) -> rusqlite::Result<Connection> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)?;
    connection.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
    Ok(connection)
}

/// Whether `name` is an ordinary table, not a view or a virtual table.
pub(crate) fn is_regular_table(connection: &Connection, name: &str) -> rusqlite::Result<bool> {
    connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1 \
         AND upper(sql) NOT LIKE '%VIRTUAL%')",
        [name],
        |row| row.get(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ordinary_tables_are_regular() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("file.sqlite");
        Connection::open(&path)
            .unwrap()
            .execute_batch(
                "CREATE TABLE plain(a); CREATE TABLE keyed(k PRIMARY KEY) WITHOUT ROWID;
                 CREATE VIEW shown AS SELECT 1; CREATE VIRTUAL TABLE listed USING fts5(a);",
            )
            .unwrap();
        let connection = open(&path).unwrap();
        assert!(is_regular_table(&connection, "plain").unwrap());
        assert!(is_regular_table(&connection, "keyed").unwrap());
        assert!(!is_regular_table(&connection, "shown").unwrap());
        assert!(!is_regular_table(&connection, "listed").unwrap());
        assert!(!is_regular_table(&connection, "missing").unwrap());
        assert!(
            connection.execute_batch("CREATE TABLE written(a)").is_err(),
            "read-only"
        );
    }
}
