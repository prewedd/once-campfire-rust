# SQLite 3.50.2 → 3.53.2 (rusqlite 0.37 → 0.40), 2026-09-30

`bench/attrib --configs native-main,native-sqlite --reps 3 --concs 1,16 --secs 5`: `native-main`
is `main` at `ab88012`, `native-sqlite` is `1693e3e` (the rusqlite upgrade alone), both native
release builds, interleaved per rep. Another project's load raised the host's 1-minute load
average to 12–14 during parts of the run, which widens some ranges; it hit both builds alike.

| Route, 16 clients | 3.50.2 req/s | 3.53.2 req/s | Change |
|---|---|---|---|
| room_show | 19,841 | 19,362 | 0.98× |
| messages_page | 23,102 | 22,722 | 0.98× |
| sidebar | 22,083 | 21,888 | 0.99× |
| search | 22,812 | 22,812 | 1.00× |
| post_message | 5,468 | 5,435 | 0.99× |

Every median is within 3.5% (0.966–1.007× across all ten cells, c=1 and c=16; the largest drop is
the sidebar at c=1, 5,870 → 5,672 req/s), which is within this host's noise at the time: the
upgrade is performance-neutral. Query plans for all 254 statements the test
suite prepares were identical except one internal membership DELETE (no Bloom filter any more).
Full tables: [`report.md`](report.md).
