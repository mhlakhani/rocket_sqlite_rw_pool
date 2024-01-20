# rocket_sqlite_rw_pool

This is a database pool based off of `rocket_sync_db_pools` which exposes low-level `rusqlite` connections and transactions. It also avoids deadlocks/errors with multiple writers by exposing separate pools for read and write connections.

Also provided:

* Safe by default CSRF protection
* Query builder for bulk inserts
* Various query helpers
* Migration support

This is provided AS-IS and is alpha-quality software. Lots of documentation and automated testing is still needed - but it works fine in some production applications already.