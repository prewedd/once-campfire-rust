```
date: 2026-09-29T19:07:15+02:00
host: 7.2.5-4-omarchy, AMD RYZEN AI MAX+ 395 w/ Radeon 8060S, 32 threads, 30GB
server cpus: 8-11 (nproc 4); loadgen cpus: 12-15; network: host
env: WEB_CONCURRENCY=3 JOB_CONCURRENCY=3 RAILS_MAX_THREADS=5 
rust extra env: 
reference image: campfire-reference:bench-898653e sha256:d91fdb852402e2d3393ca262e50b657ea0cf861fc7d4150aaf3773c91d41c2fa 2026-09-27T22:01:29.746709947+02:00
rust image: campfire-rust:bench-v0.1.2 sha256:db7ab56cf2dab595c68c76932a264f76508780e1fbcb5d825f2754857ee1aa21 2026-09-29T16:42:54.788507762Z
rust HEAD: 24c8597 (dirty: 0 files)
```

Reps: reference 3, rust 3. Cells: median [min–max].

### Startup and memory

| Metric | Rails | Rust | Rust adv. |
|---|---|---|---|
| cold start: docker run → /up 200 (ms) | 2,607 [2,572–2,720] | 143 [138–152] | 18.2× |
| idle memory.current (MB) | 355 [304–449] | 15.0 [13.0–48.0] | 23.7× |
| idle anon (MB) | 284 [284–291] | 11.0 [11.0–11.0] | 25.8× |
| peak memory.current under load (MB) | 3,479 [3,387–3,666] | 1,233 [947–1,300] | 2.8× |
| peak anon under load (MB) | 3,284 [3,233–3,368] | 449 [446–451] | 7.3× |

### HTTP (signed in as david; keep-alive; c = concurrent connections)

| Metric | Rails | Rust | Rust adv. |
|---|---|---|---|
| room_show c=1 req/s | 92.9 [90.7–93.9] | 5,096 [5,077–5,111] | 54.9× |
| room_show c=1 p50 ms | 10.4 [10.1–10.7] | 0.19 [0.19–0.19] | 53.9× |
| room_show c=1 p99 ms | 13.3 [12.9–13.4] | 0.26 [0.26–0.26] | 51.1× |
| room_show c=16 req/s | 212 [204–230] | 20,213 [20,133–20,279] | 95.5× |
| room_show c=16 p50 ms | 43.9 [42.0–69.0] | 0.77 [0.76–0.77] | 57.4× |
| room_show c=16 p99 ms | 252 [150–253] | 1.51 [1.50–1.51] | 167.6× |
| room_show c=64 req/s | 184 [167–188] | 20,106 [20,099–20,360] | 109.0× |
| room_show c=64 p50 ms | 347 [339–390] | 3.16 [3.12–3.16] | 109.9× |
| room_show c=64 p99 ms | 461 [446–473] | 5.24 [5.19–5.26] | 88.1× |
| messages_page c=1 req/s | 168 [161–169] | 5,952 [5,922–5,971] | 35.3× |
| messages_page c=1 p50 ms | 5.70 [5.56–5.73] | 0.16 [0.16–0.17] | 34.8× |
| messages_page c=1 p99 ms | 14.4 [7.8–17.6] | 0.22 [0.22–0.23] | 64.1× |
| messages_page c=16 req/s | 411 [401–413] | 23,092 [23,020–23,226] | 56.2× |
| messages_page c=16 p50 ms | 37.6 [35.4–41.9] | 0.65 [0.65–0.66] | 57.5× |
| messages_page c=16 p99 ms | 88.2 [84.3–107.2] | 1.47 [1.46–1.55] | 60.0× |
| messages_page c=64 req/s | 377 [363–382] | 23,440 [23,392–23,650] | 62.1× |
| messages_page c=64 p50 ms | 168 [166–169] | 2.66 [2.66–2.68] | 63.3× |
| messages_page c=64 p99 ms | 247 [237–253] | 4.55 [4.44–4.70] | 54.3× |
| sidebar c=1 req/s | 206 [199–212] | 5,865 [5,859–5,958] | 28.4× |
| sidebar c=1 p50 ms | 4.55 [4.48–4.65] | 0.17 [0.16–0.17] | 27.6× |
| sidebar c=1 p99 ms | 6.73 [6.62–19.79] | 0.24 [0.23–0.24] | 28.3× |
| sidebar c=16 req/s | 529 [528–544] | 22,634 [22,584–22,746] | 42.8× |
| sidebar c=16 p50 ms | 28.7 [27.7–29.7] | 0.67 [0.66–0.67] | 42.9× |
| sidebar c=16 p99 ms | 55.1 [51.6–55.3] | 1.50 [1.49–1.51] | 36.7× |
| sidebar c=64 req/s | 482 [482–544] | 23,275 [23,224–23,358] | 48.3× |
| sidebar c=64 p50 ms | 115 [114–125] | 2.71 [2.69–2.72] | 42.3× |
| sidebar c=64 p99 ms | 228 [156–243] | 4.62 [4.56–4.65] | 49.2× |
| search c=1 req/s | 168 [166–172] | 6,444 [6,426–6,463] | 38.4× |
| search c=1 p50 ms | 5.78 [5.65–5.86] | 0.15 [0.15–0.15] | 38.6× |
| search c=1 p99 ms | 7.87 [7.72–7.87] | 0.22 [0.22–0.22] | 35.1× |
| search c=16 req/s | 390 [365–390] | 23,276 [23,144–23,312] | 59.7× |
| search c=16 p50 ms | 44.4 [42.7–44.7] | 0.64 [0.64–0.65] | 69.0× |
| search c=16 p99 ms | 82.1 [75.6–82.7] | 1.50 [1.49–1.51] | 54.8× |
| search c=64 req/s | 361 [345–385] | 23,902 [23,845–24,031] | 66.2× |
| search c=64 p50 ms | 168 [165–180] | 2.61 [2.60–2.64] | 64.5× |
| search c=64 p99 ms | 272 [198–276] | 4.49 [4.37–4.55] | 60.5× |
| avatar c=1 req/s | 28,671 [28,617–29,272] | 68,471 [67,961–69,070] | 2.4× |
| avatar c=1 p50 ms | 0.03 [0.03–0.03] | 0.01 [0.01–0.01] | 2.3× |
| avatar c=1 p99 ms | 0.09 [0.09–0.09] | 0.02 [0.02–0.02] | 5.2× |
| avatar c=16 req/s | 96,631 [94,424–96,761] | 432,312 [423,435–433,682] | 4.5× |
| avatar c=16 p50 ms | 0.10 [0.10–0.10] | 0.04 [0.04–0.04] | 2.9× |
| avatar c=16 p99 ms | 0.90 [0.88–0.91] | 0.07 [0.07–0.07] | 12.7× |
| avatar c=64 req/s | 77,032 [76,489–77,415] | 444,773 [438,778–451,132] | 5.8× |
| avatar c=64 p50 ms | 0.32 [0.32–0.32] | 0.14 [0.13–0.14] | 2.3× |
| avatar c=64 p99 ms | 4.95 [4.94–4.99] | 0.31 [0.31–0.36] | 15.8× |
| static_css c=1 req/s | 35,670 [35,130–35,672] | 74,232 [73,856–74,253] | 2.1× |
| static_css c=1 p50 ms | 0.03 [0.03–0.03] | 0.01 [0.01–0.01] | 2.0× |
| static_css c=1 p99 ms | 0.07 [0.07–0.07] | 0.02 [0.02–0.02] | 3.9× |
| static_css c=16 req/s | 133,487 [131,879–133,714] | 425,836 [405,959–441,589] | 3.2× |
| static_css c=16 p50 ms | 0.08 [0.08–0.08] | 0.03 [0.03–0.04] | 2.4× |
| static_css c=16 p99 ms | 0.64 [0.64–0.67] | 0.09 [0.06–0.10] | 7.5× |
| static_css c=64 req/s | 108,441 [108,340–108,773] | 465,145 [432,072–471,852] | 4.3× |
| static_css c=64 p50 ms | 0.29 [0.29–0.29] | 0.13 [0.13–0.13] | 2.2× |
| static_css c=64 p99 ms | 3.46 [3.46–3.48] | 0.30 [0.29–0.38] | 11.6× |
| up c=1 req/s | 1,824 [1,796–1,824] | 27,671 [27,625–27,818] | 15.2× |
| up c=1 p50 ms | 0.52 [0.52–0.52] | 0.04 [0.03–0.04] | 14.8× |
| up c=1 p99 ms | 1.08 [1.03–1.31] | 0.05 [0.05–0.05] | 22.4× |
| up c=16 req/s | 4,062 [3,972–4,078] | 131,390 [129,633–134,992] | 32.3× |
| up c=16 p50 ms | 3.79 [3.79–3.90] | 0.12 [0.12–0.12] | 31.1× |
| up c=16 p99 ms | 7.96 [7.89–7.96] | 0.21 [0.21–0.21] | 37.5× |
| up c=64 req/s | 4,001 [3,923–4,058] | 134,455 [133,887–136,745] | 33.6× |
| up c=64 p50 ms | 15.8 [15.4–16.2] | 0.46 [0.45–0.46] | 34.3× |
| up c=64 p99 ms | 25.1 [24.8–25.4] | 0.96 [0.95–0.97] | 26.0× |
| post_message c=1 req/s | 140 [139–144] | 2,257 [2,250–2,274] | 16.1× |
| post_message c=1 p50 ms | 6.55 [6.42–6.57] | 0.41 [0.41–0.41] | 16.0× |
| post_message c=1 p99 ms | 13.8 [13.2–15.4] | 1.70 [1.66–1.71] | 8.1× |
| post_message c=16 req/s | 264 [254–268] | 5,494 [5,448–5,558] | 20.8× |
| post_message c=16 p50 ms | 55.6 [49.2–56.6] | 2.69 [2.65–2.70] | 20.7× |
| post_message c=16 p99 ms | 174 [140–177] | 7.11 [7.02–7.21] | 24.5× |
| post_message c=64 req/s | 258 [243–261] | 5,421 [5,390–5,462] | 21.0× |
| post_message c=64 p50 ms | 234 [211–236] | 11.6 [11.5–11.6] | 20.2× |
| post_message c=64 p99 ms | 385 [383–485] | 17.5 [17.2–17.6] | 22.0× |

### HTTP errors / non-2xx-3xx (first rep, per app)

| Metric | Rails | Rust | Rust adv. |
|---|---|---|---|
- reference: none
- rust: none

### Action Cable fan-out (one room; chatter.js subscriptions per client)

| Metric | Rails | Rust | Rust adv. |
|---|---|---|---|
| 100 clients: subscribed | 100 [100–100] | 100 [100–100] | 1.0× |
| 100 clients: connect+subscribe all (s) | 0.27 [0.27–0.27] | 0.06 [0.06–0.06] | 4.5× |
| 100 clients: paced post→one client p50 ms | 14.7 [14.4–15.1] | 2.15 [2.10–2.20] | 6.8× |
| 100 clients: paced post→all clients p50 ms | 20.1 [18.9–22.0] | 2.36 [2.26–2.40] | 8.5× |
| 100 clients: paced post→all clients p99 ms | 62.5 [58.6–102.0] | 6.89 [5.65–13.77] | 9.1× |
| 100 clients: max sustained msgs/s (delivered to all) | 78.6 [77.5–83.5] | 3,051 [3,048–3,089] | 38.8× |
| 100 clients: deliveries/s (client×message) | 7,858 [7,749–8,346] | 305,061 [304,823–308,893] | 38.8× |
| 100 clients: saturated post→all p50 ms | 45.0 [44.0–53.4] | 1.20 [1.19–1.21] | 37.4× |
| 100 clients: saturated POST p50 ms | 41.2 [31.8–43.2] | 1.19 [1.17–1.19] | 34.7× |
| 1000 clients: subscribed | 1,000 [1,000–1,000] | 1,000 [1,000–1,000] | 1.0× |
| 1000 clients: connect+subscribe all (s) | 1.49 [1.47–1.49] | 0.14 [0.12–1.14] | 10.6× |
| 1000 clients: paced post→one client p50 ms | 49.9 [44.6–50.8] | 4.14 [4.02–4.24] | 12.0× |
| 1000 clients: paced post→all clients p50 ms | 101 [99–194] | 6.43 [6.42–6.84] | 15.6× |
| 1000 clients: paced post→all clients p99 ms | 197 [192–215] | 8.44 [8.23–8.81] | 23.4× |
| 1000 clients: max sustained msgs/s (delivered to all) | 12.3 [10.4–12.6] | 513 [498–525] | 41.7× |
| 1000 clients: deliveries/s (client×message) | 12,328 [10,369–12,588] | 513,332 [497,940–525,347] | 41.6× |
| 1000 clients: saturated post→all p50 ms | 966 [567–1,367] | 15.1 [14.7–15.1] | 64.0× |
| 1000 clients: saturated POST p50 ms | 162 [137–226] | 7.37 [7.26–7.70] | 22.0× |
| 5000 clients: subscribed | 5,000 [5,000–5,000] | 5,000 [5,000–5,000] | 1.0× |
| 5000 clients: connect+subscribe all (s) | 8.65 [8.46–8.70] | 1.27 [1.21–1.33] | 6.8× |
| 5000 clients: paced post→one client p50 ms | 294 [278–1,269] | 12.0 [11.8–12.4] | 24.6× |
| 5000 clients: paced post→all clients p50 ms | 729 [667–2,654] | 21.4 [21.3–21.6] | 34.0× |
| 5000 clients: paced post→all clients p99 ms | 943 [771–3,607] | 33.6 [30.8–34.4] | 28.1× |
| 5000 clients: max sustained msgs/s (delivered to all) | 2.10 [1.70–2.50] | 119 [118–120] | 56.6× |
| 5000 clients: deliveries/s (client×message) | 10,256 [8,361–12,591] | 594,554 [589,159–602,572] | 58.0× |
| 5000 clients: saturated post→all p50 ms | 2,388 [2,284–3,426] | 124 [121–124] | 19.3× |
| 5000 clients: saturated POST p50 ms | 752 [640–1,160] | 18.3 [18.2–19.5] | 41.2× |
| 10000 clients: subscribed | 10,000 [9,999–10,000] | 10,000 [10,000–10,000] | 1.0× |
| 10000 clients: connect+subscribe all (s) | 29.2 [16.4–29.4] | 1.59 [1.51–1.64] | 18.4× |
| 10000 clients: paced post→one client p50 ms | 973 [904–1,193] | 21.5 [21.5–21.5] | 45.4× |
| 10000 clients: paced post→all clients p50 ms | 3,635 [2,472–5,030] | 40.0 [38.3–40.0] | 91.0× |
| 10000 clients: paced post→all clients p99 ms | 5,489 [4,065–6,578] | 57.1 [55.0–62.0] | 96.2× |
| 10000 clients: max sustained msgs/s (delivered to all) | 0.90 [0.90–1.00] | 65.6 [61.3–65.8] | 72.9× |
| 10000 clients: deliveries/s (client×message) | 9,485 [8,922–9,784] | 656,183 [613,090–657,503] | 69.2× |
| 10000 clients: saturated post→all p50 ms | 4,342 [3,248–5,415] | 294 [287–299] | 14.8× |
| 10000 clients: saturated POST p50 ms | 2,640 [2,255–2,982] | 25.4 [24.4–26.0] | 104.0× |

### Upload + thumbnail (black_hole.jpg, 505 KB)

| Metric | Rails | Rust | Rust adv. |
|---|---|---|---|
| POST with attachment (ms) | 122 [103–136] | 28.9 [28.6–30.1] | 4.2× |
| then GET thumb → 200 (ms) | 0.40 [0.40–0.50] | 0.30 [0.30–0.30] | 1.3× |
| POST → thumbnail served (ms) | 122 [103–136] | 29.1 [28.9–30.3] | 4.2× |

### Memory during cable fan-out, by process (MB, peak within the phase)

App process: Rails' Puma master and workers (Action Cable runs in them), or Rust's one campfire
process (its front server included). Pss counts pages shared between forked workers once;
RssAnon counts them in every process.

| Metric | Rails | Rust | Rust adv. |
|---|---|---|---|
| 100 clients, all subscribed, idle: app process Pss | 544 [541–563] | 168 [163–177] | 3.2× |
| 100 clients, all subscribed, idle: app process RssAnon | 691 [684–710] | 144 [139–154] | 4.8× |
| 100 clients, all subscribed, idle: app + Redis + Thruster Pss | 583 [580–602] | 168 [163–177] | 3.5× |
| 100 clients, all subscribed, idle: whole container Pss | 879 [878–882] | 168 [163–177] | 5.2× |
| 100 clients, saturated fan-out: app process Pss | 664 [639–707] | 173 [165–180] | 3.8× |
| 100 clients, saturated fan-out: app process RssAnon | 810 [781–852] | 149 [141–156] | 5.4× |
| 100 clients, saturated fan-out: app + Redis + Thruster Pss | 715 [690–758] | 173 [165–180] | 4.1× |
| 100 clients, saturated fan-out: whole container Pss | 992 [991–1,059] | 173 [165–180] | 5.7× |
| 1000 clients, all subscribed, idle: app process Pss | 656 [627–676] | 186 [176–191] | 3.5× |
| 1000 clients, all subscribed, idle: app process RssAnon | 800 [767–820] | 161 [151–166] | 5.0× |
| 1000 clients, all subscribed, idle: app + Redis + Thruster Pss | 754 [726–774] | 186 [176–191] | 4.1× |
| 1000 clients, all subscribed, idle: whole container Pss | 1,031 [1,027–1,075] | 186 [176–191] | 5.6× |
| 1000 clients, saturated fan-out: app process Pss | 903 [870–929] | 186 [176–190] | 4.9× |
| 1000 clients, saturated fan-out: app process RssAnon | 1,042 [1,014–1,072] | 161 [151–166] | 6.5× |
| 1000 clients, saturated fan-out: app + Redis + Thruster Pss | 1,011 [978–1,035] | 186 [176–190] | 5.4× |
| 1000 clients, saturated fan-out: whole container Pss | 1,310 [1,255–1,335] | 186 [176–190] | 7.1× |
| 5000 clients, all subscribed, idle: app process Pss | 1,000 [979–1,011] | 239 [239–254] | 4.2× |
| 5000 clients, all subscribed, idle: app process RssAnon | 1,142 [1,116–1,152] | 214 [214–229] | 5.3× |
| 5000 clients, all subscribed, idle: app + Redis + Thruster Pss | 1,374 [1,356–1,387] | 239 [239–254] | 5.8× |
| 5000 clients, all subscribed, idle: whole container Pss | 1,661 [1,654–1,672] | 239 [239–254] | 7.0× |
| 5000 clients, saturated fan-out: app process Pss | 1,702 [1,260–1,763] | 239 [239–252] | 7.1× |
| 5000 clients, saturated fan-out: app process RssAnon | 1,844 [1,400–1,900] | 214 [214–227] | 8.6× |
| 5000 clients, saturated fan-out: app + Redis + Thruster Pss | 2,111 [1,672–2,176] | 239 [239–252] | 8.8× |
| 5000 clients, saturated fan-out: whole container Pss | 2,406 [1,943–2,471] | 239 [239–252] | 10.1× |
| 10000 clients, all subscribed, idle: app process Pss | 1,469 [1,454–1,531] | 327 [317–342] | 4.5× |
| 10000 clients, all subscribed, idle: app process RssAnon | 1,608 [1,589–1,673] | 302 [292–317] | 5.3× |
| 10000 clients, all subscribed, idle: app + Redis + Thruster Pss | 2,325 [2,204–2,568] | 327 [317–342] | 7.1× |
| 10000 clients, all subscribed, idle: whole container Pss | 2,619 [2,475–2,862] | 327 [317–342] | 8.0× |
| 10000 clients, saturated fan-out: app process Pss | 2,199 [1,972–2,213] | 324 [319–342] | 6.8× |
| 10000 clients, saturated fan-out: app process RssAnon | 2,333 [2,114–2,352] | 299 [294–317] | 7.8× |
| 10000 clients, saturated fan-out: app + Redis + Thruster Pss | 3,047 [3,017–3,123] | 324 [319–342] | 9.4× |
| 10000 clients, saturated fan-out: whole container Pss | 3,340 [3,288–3,415] | 324 [319–342] | 10.3× |
