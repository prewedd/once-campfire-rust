```
date: 2026-09-30T00:42:37+0200
host: 7.2.5-4-omarchy, 32 threads
server cpus: 8-11; loadgen cpus: 12-15
image: campfire-rust:app sha256:7042897243ae9e9f6fc47c3e18d0ae00165de1ad8a3488369bd79d01a8d46ac7
native-main: /home/dhh/Work/basecamp/once-campfire-rust/target/prof-main/release/campfire preload=
native-sqlite: /home/dhh/Work/basecamp/once-campfire-rust/target/prof-sqlite/release/campfire preload=
env: WEB_CONCURRENCY=3 RAILS_MAX_THREADS=5 JOB_CONCURRENCY=3
routes: room_show,messages_page,sidebar,search,post_message; concs: 1,16; secs: 5; cable: ; app env:
```

Reps per configuration: native-main 3, native-sqlite 3. Cells: median [min–max]; (×) is the gain over native-main (>1 is better).
Host load (1-min loadavg at start of each run): native-main 2.13/13.83/8.04, native-sqlite 12.35/11.13/8.04

### HTTP room_show

| Metric | native-main | native-sqlite |
|---|---|---|
| c=1 req/s | 4,977 [4,488–4,991] | 4,946 [2,744–4,995] (0.99×) |
| c=1 p50 ms | 0.20 [0.20–0.20] | 0.20 [0.20–0.20] (0.99×) |
| c=1 p99 ms | 0.28 [0.28–0.78] | 0.28 [0.27–2.66] (0.99×) |
| c=16 req/s | 19,841 [12,854–19,894] | 19,362 [13,626–19,614] (0.98×) |
| c=16 p50 ms | 0.78 [0.78–0.94] | 0.80 [0.79–0.91] (0.98×) |
| c=16 p99 ms | 1.57 [1.52–4.85] | 1.57 [1.56–3.93] (1.00×) |
| c=1 campfire CPU ms/req | 0.20 [0.20–0.21] | 0.20 [0.20–0.26] (0.99×) |
| c=1 thrust CPU ms/req | – | – |
| c=1 docker-proxy CPU ms/req | – | – |
| c=1 loadgen cores busy | 0.050 [0.050–0.050] | 0.050 [0.050–0.050] |
| c=1 campfire cores busy | 0.97 [0.94–0.98] | 0.98 [0.72–0.98] |
| c=16 campfire CPU ms/req | 0.19 [0.19–0.26] | 0.20 [0.19–0.25] (0.98×) |
| c=16 thrust CPU ms/req | – | – |
| c=16 docker-proxy CPU ms/req | – | – |
| c=16 loadgen cores busy | 0.22 [0.21–0.22] | 0.21 [0.21–0.21] |
| c=16 campfire cores busy | 3.79 [3.28–3.82] | 3.80 [3.36–3.80] |
| avg response bytes | 24,231 [24,231–24,231] | 24,231 [24,231–24,231] |

### HTTP messages_page

| Metric | native-main | native-sqlite |
|---|---|---|
| c=1 req/s | 5,785 [4,365–5,860] | 5,750 [5,694–5,904] (0.99×) |
| c=1 p50 ms | 0.17 [0.17–0.18] | 0.17 [0.17–0.17] (0.99×) |
| c=1 p99 ms | 0.25 [0.24–1.46] | 0.23 [0.23–0.26] (1.05×) |
| c=16 req/s | 23,102 [10,082–23,115] | 22,722 [22,654–22,943] (0.98×) |
| c=16 p50 ms | 0.66 [0.65–1.41] | 0.66 [0.66–0.67] (0.99×) |
| c=16 p99 ms | 1.49 [1.46–4.45] | 1.50 [1.48–1.52] (0.99×) |
| c=1 campfire CPU ms/req | 0.17 [0.17–0.20] | 0.17 [0.17–0.17] (0.99×) |
| c=1 thrust CPU ms/req | – | – |
| c=1 docker-proxy CPU ms/req | – | – |
| c=1 loadgen cores busy | 0.050 [0.050–0.060] | 0.050 [0.050–0.050] |
| c=1 campfire cores busy | 0.99 [0.88–0.99] | 0.99 [0.99–1.00] |
| c=16 campfire CPU ms/req | 0.16 [0.16–0.31] | 0.16 [0.16–0.16] (0.99×) |
| c=16 thrust CPU ms/req | – | – |
| c=16 docker-proxy CPU ms/req | – | – |
| c=16 loadgen cores busy | 0.25 [0.24–0.25] | 0.24 [0.24–0.24] |
| c=16 campfire cores busy | 3.70 [3.12–3.73] | 3.71 [3.70–3.71] |
| avg response bytes | 16,158 [16,158–16,158] | 16,158 [16,158–16,158] |

### HTTP sidebar

| Metric | native-main | native-sqlite |
|---|---|---|
| c=1 req/s | 5,870 [5,556–5,984] | 5,672 [2,758–5,852] (0.97×) |
| c=1 p50 ms | 0.16 [0.16–0.17] | 0.17 [0.16–0.19] (0.95×) |
| c=1 p99 ms | 0.25 [0.24–0.28] | 0.25 [0.24–2.60] (1.00×) |
| c=16 req/s | 22,083 [21,837–22,141] | 21,888 [19,242–21,976] (0.99×) |
| c=16 p50 ms | 0.69 [0.68–0.69] | 0.69 [0.69–0.73] (1.00×) |
| c=16 p99 ms | 1.52 [1.52–1.54] | 1.54 [1.53–2.50] (0.99×) |
| c=1 campfire CPU ms/req | 0.17 [0.16–0.17] | 0.17 [0.17–0.27] (0.97×) |
| c=1 thrust CPU ms/req | – | – |
| c=1 docker-proxy CPU ms/req | – | – |
| c=1 loadgen cores busy | 0.050 [0.050–0.060] | 0.050 [0.050–0.060] |
| c=1 campfire cores busy | 0.97 [0.97–0.98] | 0.97 [0.75–0.97] |
| c=16 campfire CPU ms/req | 0.17 [0.17–0.17] | 0.17 [0.17–0.18] (1.00×) |
| c=16 thrust CPU ms/req | – | – |
| c=16 docker-proxy CPU ms/req | – | – |
| c=16 loadgen cores busy | 0.23 [0.22–0.23] | 0.22 [0.22–0.22] |
| c=16 campfire cores busy | 3.69 [3.67–3.70] | 3.67 [3.54–3.68] |
| avg response bytes | 5,910 [5,910–5,910] | 5,910 [5,910–5,910] |

### HTTP search

| Metric | native-main | native-sqlite |
|---|---|---|
| c=1 req/s | 6,318 [5,577–6,412] | 6,365 [5,997–6,460] (1.01×) |
| c=1 p50 ms | 0.15 [0.15–0.15] | 0.15 [0.15–0.16] (1.01×) |
| c=1 p99 ms | 0.24 [0.22–0.36] | 0.24 [0.22–0.26] (1.01×) |
| c=16 req/s | 22,812 [14,513–22,923] | 22,812 [17,213–22,828] (1.00×) |
| c=16 p50 ms | 0.66 [0.66–0.78] | 0.66 [0.66–0.74] (1.00×) |
| c=16 p99 ms | 1.50 [1.49–4.87] | 1.51 [1.49–3.04] (0.99×) |
| c=1 campfire CPU ms/req | 0.16 [0.16–0.17] | 0.16 [0.16–0.17] (1.01×) |
| c=1 thrust CPU ms/req | – | – |
| c=1 docker-proxy CPU ms/req | – | – |
| c=1 loadgen cores busy | 0.060 [0.060–0.060] | 0.060 [0.060–0.060] |
| c=1 campfire cores busy | 1.00 [0.95–1.00] | 1.01 [1.01–1.01] |
| c=16 campfire CPU ms/req | 0.16 [0.16–0.21] | 0.16 [0.16–0.20] (1.00×) |
| c=16 thrust CPU ms/req | – | – |
| c=16 docker-proxy CPU ms/req | – | – |
| c=16 loadgen cores busy | 0.24 [0.22–0.24] | 0.23 [0.23–0.24] |
| c=16 campfire cores busy | 3.66 [3.08–3.70] | 3.66 [3.44–3.70] |
| avg response bytes | 9,766 [9,766–9,766] | 9,766 [9,766–9,766] |

### HTTP post_message

| Metric | native-main | native-sqlite |
|---|---|---|
| c=1 req/s | 2,111 [1,595–2,117] | 2,102 [1,402–2,163] (1.00×) |
| c=1 p50 ms | 0.44 [0.44–0.48] | 0.44 [0.43–0.46] (0.99×) |
| c=1 p99 ms | 1.79 [1.68–3.44] | 1.75 [1.72–5.55] (1.02×) |
| c=16 req/s | 5,468 [5,394–5,477] | 5,435 [5,369–5,453] (0.99×) |
| c=16 p50 ms | 2.73 [2.71–2.77] | 2.73 [2.72–2.74] (1.00×) |
| c=16 p99 ms | 7.10 [6.83–7.21] | 7.25 [7.05–7.51] (0.98×) |
| c=1 campfire CPU ms/req | 0.52 [0.52–0.61] | 0.52 [0.51–0.63] (1.00×) |
| c=1 thrust CPU ms/req | – | – |
| c=1 docker-proxy CPU ms/req | – | – |
| c=1 loadgen cores busy | 0.030 [0.030–0.040] | 0.030 [0.030–0.040] |
| c=1 campfire cores busy | 1.10 [0.98–1.10] | 1.09 [0.88–1.10] |
| c=16 campfire CPU ms/req | 0.54 [0.53–0.54] | 0.54 [0.54–0.54] (1.00×) |
| c=16 thrust CPU ms/req | – | – |
| c=16 docker-proxy CPU ms/req | – | – |
| c=16 loadgen cores busy | 0.090 [0.090–0.090] | 0.090 [0.090–0.090] |
| c=16 campfire cores busy | 2.92 [2.89–2.95] | 2.91 [2.90–2.92] |
| avg response bytes | 1,992 [1,992–1,992] | 1,992 [1,991–1,992] |
