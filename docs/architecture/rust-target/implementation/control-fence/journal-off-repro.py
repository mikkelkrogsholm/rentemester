import sqlite3, tempfile, os
with tempfile.TemporaryDirectory(prefix='rm-fence-off-') as path:
    db = sqlite3.connect(os.path.join(path, 'db'), isolation_level=None)
    print('sqlite', sqlite3.sqlite_version)
    print('journal_mode', db.execute('PRAGMA journal_mode=OFF').fetchone())
    db.execute('PRAGMA synchronous=FULL')
    db.execute('PRAGMA cache_size=1')
    db.execute('CREATE TABLE business(id INTEGER PRIMARY KEY,payload TEXT)')
    db.execute('BEGIN IMMEDIATE')
    db.executemany('INSERT INTO business(payload) VALUES(?)', [('old'+'x'*4000,)]*1000)
    db.execute('COMMIT')
    print('synchronous', db.execute('PRAGMA synchronous').fetchone())
    db.execute('BEGIN IMMEDIATE')
    db.execute("UPDATE business SET payload='new'||substr(payload,4)")
    db.execute('ROLLBACK')
    changed = db.execute("SELECT COUNT(*) FROM business WHERE payload LIKE 'new%'").fetchone()[0]
    print('new rows after successful rollback', changed)
    print('integrity', db.execute('PRAGMA quick_check').fetchone())
    assert changed > 0, 'This SQLite environment did not reproduce the undefined OFF rollback'
    db.close()
