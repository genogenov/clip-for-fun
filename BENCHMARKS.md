# Benchmarks

These benchmarks check that clip-for-fun is at least as fast as the great wl-clipboard, which is already plenty fast for all practical intents and purposes.

Median times and memory measured with [`bench/bench.sh`](bench/bench.sh), which compares clip-for-fun directly against wl-clipboard by running both with [hyperfine](https://github.com/sharkdp/hyperfine) against a private headless sway. Every row is the same behavior in both tools: copies offer the same five text types without type detection, and each paste reads from the same tool's copy.

| | clip-for-fun time | wl-clipboard time | clip-for-fun memory | wl-clipboard memory |
| --- | ---: | ---: | ---: | ---: |
| `copy` text argument | 2.10 ms | 2.31 ms | 0.9 MiB | 0.6 MiB |
| `copy` 40 B from stdin | 2.08 ms | 3.74 ms | 0.9 MiB | 0.6 MiB |
| `copy` 100 MiB from stdin | 31.61 ms | 33.71 ms | 1.0 MiB | 0.6 MiB |
| `paste` 40 B | 0.87 ms | 1.58 ms | 1.9 MiB | 3.6 MiB |
| `paste` 100 MiB to `/dev/null` | 2.59 ms | 4.59 ms | 1.9 MiB | 3.6 MiB |
| `paste` 100 MiB to a file | 21.01 ms | 22.93 ms | 1.9 MiB | 3.6 MiB |

Memory is resident memory (RSS). For copies it is the background process that keeps the copy available until the next copy; for pastes it is the median peak of 5 runs, and for wl-paste, which runs `cat`, the larger of the two processes.

100 runs per case (20 for 100 MiB) on an Intel Core i9-14900HX pinned to its performance cores, Linux 7.2.8, sway 1.12, wl-clipboard 2.3.0, hyperfine 1.20.0.

To reproduce, install sway 1.11+, wl-clipboard, hyperfine and GNU time, then run `bench/bench.sh`, pinned to fast cores with `taskset` on hybrid CPUs (`taskset -c 0-15 bench/bench.sh` here). `bench/bench.sh --help` lists the options, including `--compositor host` to use the running compositor.
