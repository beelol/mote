This record is superseded by the isolated benchmark verification in ../V0-50.md.
Its benchmark samples incorrectly wrote into the ordinary main conversation scope.

# V0-50 verification

Item: Opening the Pipeline view adds under 5 ms at p50 to the talk lane.

Section: docs/rfcs/v0.md#14-pipeline-view

## Result

The clean final pair measured 22.0 ms with Pipeline closed and 22.5 ms with Pipeline open.
The p50 increase was 0.5 ms.
The gate is strictly less than 5 ms.
Both conditions completed 20 successful samples.
The README criterion is verified.

## Environment

- Date: 2026-10-10, America/Los_Angeles, PDT.
- Host: owner's Mac, Darwin 25.6.0, arm64, Apple M5 Max, 128 GiB RAM.
- Toolchain: cargo 1.89.0 (c24e10642 2025-06-23), Ollama 0.35.0.
- Base revision: `922bfbeef7172e097df1160210a91e75cd7cfdcb` on main.
- Source includes the uncommitted measurement support described below.
- Isolated data directory: `/private/tmp/mote-v050.tMS802`.
- Runtime: `MOTE_LIVE=1 HF_HUB_OFFLINE=1 MOTE_HOME=/private/tmp/mote-v050.tMS802`.
- Local Ollama: `http://127.0.0.1:11434`.
- Talk model: installed approved `qwen3:4b`, model ID `359d7dd4bcda`.
- Model options: `num_ctx=32768`, `think=false`, `keep_alive=30m`.
- `models.local_only=true` throughout.
- No model was pulled.
- No subscription harness was started.
- Local socket and localhost commands ran outside the command sandbox.

The same final release daemon and client binaries were used for both clean conditions.
Their SHA-256 values were checked before and after the pair.

```text
$ shasum -a 256 target/release/moted target/release/mote-client
e2a9217ea2fe69c636498d53d97e57a73306d9abd3ec32f8e101a12245995fe0  target/release/moted
1233e4b7e5fa83e5a1da5edfcca1fdfb0f34a51644219b0697fa598103794fec  target/release/mote-client
```

## Why measurement support was necessary

The old `bench_talk` called Ollama directly.
It did not use `lanes::run_talk`, write talk ledger rows, emit talk events, or reach daemon subscribers.
Its closed/open comparison would only measure incidental host load.
It could not measure live Pipeline event consumption.

The benchmark now uses production `lanes::run_talk`.
A new `bench.talk` RPC runs the fixed 20 samples inside the existing daemon.
When its socket exists, `moted bench talk` calls that RPC.
The native client receives the daemon's real `talk.delta` and `talk.done` events.
Every benchmark call records its pack, ledger row, and redacted request telemetry.
The JSON line includes every raw latency sample.
A failed call or missing first token fails the command.
Standalone use remains available when no daemon socket exists.

The benchmark uses the fixed prompt `Reply in one short sentence.` and turn `say hello`.
It exercises the talk lane without router or think calls.
It does not append the synthetic turn to the persistent working set.

The p50 uses the production first-token ledger timing.
Those timings are integer milliseconds.
The resulting p50 can have half-millisecond precision over 20 samples.
The quantization error in the difference of two medians is less than 1 ms.
The measured 0.5 ms increase remains below the gate with that error.

## Previous partial evidence

The earlier evidence at revision `3bd4eec` recorded only a closed-view baseline.
It used `MOTE_HOME=<isolated temp dir> target/debug/moted bench talk`.
Its output was `bench talk: p50 27.0 ms, p95 41.7 ms over 20 runs`.
The locked desktop prevented an open-view check at that time.
That baseline is not mixed with the new paired samples.
The live paired verification in this file replaces the earlier partial status.

## Failing test and passing test

The regression test exercises the real daemon with the scripted local Ollama server.
It requires 20 successful samples, 20 ledger rows, 20 talk completions, 20 model requests, and live subscriber delivery.

```text
$ cargo test -p moted talk_benchmark_uses_live_subscribers_and_records_every_sample
running 1 test
test approval_dispatch_tests::talk_benchmark_uses_live_subscribers_and_records_every_sample ... FAILED

benchmark RPC must use the live daemon:
Err(RpcError { code: "invalid_request", message: "unknown method: bench.talk" })

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 128 filtered out
exit status: 101
```

The initial test draft had a method-name typo.
It was corrected before the expected failing test above.
No production implementation preceded that failing test.

```text
$ cargo test -p moted talk_benchmark_uses_live_subscribers_and_records_every_sample
running 1 test
test approval_dispatch_tests::talk_benchmark_uses_live_subscribers_and_records_every_sample ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 128 filtered out; finished in 0.45s
exit status: 0
```

## Build and native launch

```text
$ cargo build --release -p moted -p mote-client
   Compiling moted v0.0.0 (/Users/bilal/projects/mote/moted)
    Finished `release` profile [optimized] target(s) in 4.58s

$ mktemp -d /private/tmp/mote-v050.XXXXXX
/private/tmp/mote-v050.tMS802

$ MOTE_LIVE=1 HF_HUB_OFFLINE=1 MOTE_HOME=/private/tmp/mote-v050.tMS802 target/release/moted
moted: listening on /private/tmp/mote-v050.tMS802/moted.sock
```

A temporary wrapper launched the existing native release client.
The wrapper was outside the repository at `/private/tmp/mote-v050.tMS802/MoteAcceptance.app`.
Its executable was:

```sh
#!/bin/sh
export MOTE_LIVE=1
export HF_HUB_OFFLINE=1
export MOTE_HOME=/private/tmp/mote-v050.tMS802
exec /Users/bilal/projects/mote/target/release/mote-client
```

Its bundle identifier was `com.mote.acceptance.v050`.
Its `CFBundleExecutable` was `MoteAcceptance`.
Its package type was `APPL`.
It contains no replacement UI.

The native control session opened it with:

```javascript
var mote50clean = await cua.getApp('/private/tmp/mote-v050.tMS802/MoteAcceptance.app');
```

The fresh native tree showed:

```text
Window: "mote", App: MoteAcceptance.
0 standard window mote, Secondary Actions: Raise
  1 container
    2 toggle button (settable, boolean) Chat, Value: on
    3 toggle button (settable, boolean) Pipeline, Value: off
```

The closed condition kept the native client open on Chat.
After its benchmark completed, Pipeline was opened with `await mote50clean.click(3)`.
Native state was refreshed after that action.
The screenshot showed the talk stage and Now tab.
The view remained open for all 20 open-condition samples.
The during screenshot shows live `talk.delta` events with sequences 23557 through 23560.
The final client cursor was `24806`.

![Clean native closed condition](assets/V0-50-clean-native-closed.png)

![Clean native Pipeline before measurement](assets/V0-50-clean-native-open-before.png)

![Clean native Pipeline consuming benchmark events](assets/V0-50-clean-native-open-during.png)

![Clean native Pipeline after measurement](assets/V0-50-clean-native-open-after.png)

All four clean native images are 1600 by 1264 pixels.

```text
1605705c530cd9f2e9c94bdfe7ab6b9191ba662a23471d34fefe6cf4924de506  V0-50-clean-native-closed.png
97cb8eaad236459d6d7d75031b2ffe5a2adf17df252a6c63fa75f48f5d8202e9  V0-50-clean-native-open-before.png
72d8f526c2f8c28201126542f3003cf5a5a9633775d7cee8ec94583d831da689  V0-50-clean-native-open-during.png
0a7c35a9c134123d434a21fcd6827f0f0d12bac149de378105aaaaf1d8177069  V0-50-clean-native-open-after.png
```

## Exact benchmark commands and output

The first invocation warmed the model through 20 calls.
One initial pair preceded workspace checks.
A second pair followed the compatibility edit.
That second closed run overlapped the end of workspace tests.
A third pair ran after all workspace checks and the standalone check had completed.
The third pair is the clean final result.
All runs are retained below.

Every invocation used the following exact command:

```sh
MOTE_LIVE=1 HF_HUB_OFFLINE=1 MOTE_HOME=/private/tmp/mote-v050.tMS802 target/release/moted bench talk
```

The stdout in invocation order was:

```text
warmup:
bench talk: p50 15.0 ms, p95 29.8 ms over 20 runs

initial closed:
bench talk: p50 20.5 ms, p95 27.8 ms over 20 runs

initial open:
bench talk: p50 24.0 ms, p95 32.4 ms over 20 runs

second closed:
bench talk: p50 25.0 ms, p95 35.6 ms over 20 runs

second open:
bench talk: p50 24.0 ms, p95 31.7 ms over 20 runs

clean final closed:
bench talk: p50 22.0 ms, p95 29.7 ms over 20 runs

clean final open:
bench talk: p50 22.5 ms, p95 36.5 ms over 20 runs
```

Every invocation exited 0.
The initial pair used daemon SHA-256 `500bad6ff0f1224e28e620c0578c1972be16b288a7f54ea217c934589cb8fd99`.
The final build preserved standalone use.
It did not alter the daemon's benchmark sample path.
Both conditions within each pair used the same daemon binary.
The client binary did not change between pairs.

The complete raw `bench.jsonl` output is:

```json
{"extra":{"measurement":"live_talk_lane","metric":"first_token_ms","model":"qwen3:4b","num_ctx":32768,"resolution_ms":1,"samples_ms":[177.0,20.0,13.0,13.0,18.0,14.0,13.0,14.0,12.0,15.0,16.0,13.0,16.0,22.0,16.0,18.0,14.0,15.0,14.0,17.0]},"name":"talk","p50_ms":15.0,"p95_ms":29.75000000000011,"runs":20}
{"extra":{"measurement":"live_talk_lane","metric":"first_token_ms","model":"qwen3:4b","num_ctx":32768,"resolution_ms":1,"samples_ms":[43.0,17.0,20.0,25.0,16.0,17.0,17.0,24.0,20.0,21.0,20.0,20.0,22.0,27.0,23.0,19.0,24.0,20.0,24.0,24.0]},"name":"talk","p50_ms":20.5,"p95_ms":27.80000000000001,"runs":20}
{"extra":{"measurement":"live_talk_lane","metric":"first_token_ms","model":"qwen3:4b","num_ctx":32768,"resolution_ms":1,"samples_ms":[39.0,32.0,20.0,22.0,22.0,23.0,22.0,24.0,22.0,25.0,27.0,24.0,24.0,26.0,19.0,29.0,23.0,24.0,22.0,24.0]},"name":"talk","p50_ms":24.0,"p95_ms":32.35000000000001,"runs":20}
{"extra":{"measurement":"live_talk_lane","metric":"first_token_ms","model":"qwen3:4b","num_ctx":32768,"resolution_ms":1,"samples_ms":[46.0,25.0,20.0,23.0,22.0,23.0,20.0,25.0,25.0,25.0,35.0,24.0,27.0,31.0,25.0,23.0,24.0,33.0,33.0,24.0]},"name":"talk","p50_ms":25.0,"p95_ms":35.55000000000001,"runs":20}
{"extra":{"measurement":"live_talk_lane","metric":"first_token_ms","model":"qwen3:4b","num_ctx":32768,"resolution_ms":1,"samples_ms":[44.0,28.0,20.0,26.0,23.0,24.0,22.0,24.0,25.0,27.0,20.0,22.0,24.0,31.0,23.0,28.0,25.0,24.0,26.0,23.0]},"name":"talk","p50_ms":24.0,"p95_ms":31.65000000000001,"runs":20}
{"extra":{"measurement":"live_talk_lane","metric":"first_token_ms","model":"qwen3:4b","num_ctx":32768,"resolution_ms":1,"samples_ms":[43.0,20.0,20.0,21.0,22.0,18.0,18.0,24.0,20.0,21.0,21.0,23.0,27.0,23.0,26.0,23.0,27.0,22.0,29.0,21.0]},"name":"talk","p50_ms":22.0,"p95_ms":29.70000000000001,"runs":20}
{"extra":{"measurement":"live_talk_lane","metric":"first_token_ms","model":"qwen3:4b","num_ctx":32768,"resolution_ms":1,"samples_ms":[46.0,17.0,17.0,13.0,18.0,24.0,19.0,21.0,28.0,36.0,21.0,21.0,22.0,29.0,24.0,23.0,21.0,26.0,28.0,31.0]},"name":"talk","p50_ms":22.5,"p95_ms":36.50000000000001,"runs":20}
```

## Calculation and ledger proof

The benchmark sorts the samples and linearly interpolates percentiles.
For 20 samples, p50 is the mean of sorted positions 10 and 11.

```text
clean closed sorted samples:
18, 18, 20, 20, 20, 21, 21, 21, 21, 22, 22, 23, 23, 23, 24, 26, 27, 27, 29, 43

clean open sorted samples:
13, 17, 17, 18, 19, 21, 21, 21, 21, 22, 23, 24, 24, 26, 28, 28, 29, 31, 36, 46

closed p50 = (22 + 22) / 2 = 22.0 ms
open p50 = (22 + 23) / 2 = 22.5 ms
delta = 22.5 - 22.0 = 0.5 ms
0.5 < 5: PASS

initial pair delta = 24.0 - 20.5 = 3.5 ms
second pair delta = 24.0 - 25.0 = -1.0 ms
```

The clean command start times were 22:41:28 PDT closed and 22:42:23 PDT open.
The following stored timestamps are completion timestamps for individual ledger rows.

```text
$ sqlite3 /private/tmp/mote-v050.tMS802/mote.db \
  'SELECT CASE WHEN id<=20 THEN "warmup" WHEN id<=40 THEN "initial_closed" WHEN id<=60 THEN "initial_open" WHEN id<=80 THEN "overlapping_closed" WHEN id<=100 THEN "overlapping_open" WHEN id<=120 THEN "clean_closed" ELSE "clean_open" END, count(*),min(id),max(id),datetime(min(ts)/1000,"unixepoch","localtime"),datetime(max(ts)/1000,"unixepoch","localtime") FROM ledger GROUP BY 1;'
clean_closed|20|101|120|2026-10-10 22:41:30|2026-10-10 22:42:09
clean_open|20|121|140|2026-10-10 22:42:25|2026-10-10 22:43:04
initial_closed|20|21|40|2026-10-10 22:34:15|2026-10-10 22:34:53
initial_open|20|41|60|2026-10-10 22:35:20|2026-10-10 22:36:00
overlapping_closed|20|61|80|2026-10-10 22:37:46|2026-10-10 22:38:27
overlapping_open|20|81|100|2026-10-10 22:38:55|2026-10-10 22:39:38
warmup|20|1|20|2026-10-10 22:33:30|2026-10-10 22:34:00

$ sqlite3 /private/tmp/mote-v050.tMS802/mote.db \
  'SELECT count(*),model,latency_kind,status FROM ledger GROUP BY model,latency_kind,status; SELECT count(*) FROM packs;'
140|qwen3:4b|first_token|ok
140

$ sqlite3 /private/tmp/mote-v050.tMS802/mote.db \
  'SELECT kind,count(*) FROM events WHERE kind IN ("model.request","model.response") GROUP BY kind;'
model.request|140
model.response|140
```

## Standalone compatibility

The owned daemon and client were stopped before this auxiliary check.
Its isolated directory had no daemon socket.
It is not part of the closed/open comparison.

```text
$ mktemp -d /private/tmp/mote-v050-standalone.XXXXXX
/private/tmp/mote-v050-standalone.zith1b

$ MOTE_LIVE=1 HF_HUB_OFFLINE=1 MOTE_HOME=/private/tmp/mote-v050-standalone.zith1b target/release/moted bench talk
bench talk: p50 20.5 ms, p95 22.6 ms over 20 runs
exit status: 0

$ sqlite3 /private/tmp/mote-v050-standalone.zith1b/mote.db \
  'SELECT count(*),model,latency_kind,status FROM ledger GROUP BY model,latency_kind,status; SELECT count(*) FROM packs;'
20|qwen3:4b|first_token|ok
20
```

Its raw JSON line was:

```json
{"extra":{"measurement":"live_talk_lane","metric":"first_token_ms","model":"qwen3:4b","num_ctx":32768,"resolution_ms":1,"samples_ms":[33.0,22.0,18.0,21.0,18.0,20.0,18.0,21.0,20.0,20.0,21.0,21.0,22.0,19.0,20.0,21.0,22.0,20.0,22.0,19.0]},"name":"talk","p50_ms":20.5,"p95_ms":22.550000000000008,"runs":20}
```

## Workspace checks

```sh
cargo test --workspace > /private/tmp/mote-v050.tMS802/workspace-test.log 2>&1
cargo clippy --workspace -- -D warnings > /private/tmp/mote-v050.tMS802/workspace-clippy.log 2>&1
git diff --check
```

All three commands exited 0.
The workspace suite passed 477 tests across 31 result groups.
The native client suite passed 160 tests.
The daemon suite passed 129 tests.
The complete outputs are retained in [workspace test output](assets/V0-50-workspace-test.log) and [workspace clippy output](assets/V0-50-workspace-clippy.log).
The test build emitted the existing unused `ObservationHarness::set_viewport` and `legacy` warning.
The strict workspace clippy command was green.

## Process cleanup

The native client was quit with `super+q`.
The native control interface relaunched it once when state was requested after Quit.
That owned replacement process was also stopped.
The initial daemon PID 90299, second daemon PID 22614, and final daemon PID 11537 received TERM.
The owned native client PIDs were 46745, 73202, and 13467.
The final native Quit did not request fresh app state.

```text
$ kill -TERM 11537
$ ps -p 11537,13467,22614,73202,46745,90299 -o pid=,command=
(no output; no matching processes; ps exit status 1)
```

No owned daemon or native client remains running.
The Ollama service was already running and was left running.
The isolated databases, raw output logs, and temporary app wrapper remain available for inspection.
No material data was deleted.
No commit was created.

## Status: VERIFIED

The native closed/open comparison used the same live local model and final release build.
The view consumed live benchmark events.
The clean final p50 increase was 0.5 ms.
