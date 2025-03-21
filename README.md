# slang-timings

## Basic Usage

```sh
# Run entire test suite
$ cargo run
# Run specific test case(s) by name
$ cargo run -- WeightedPool Uniswap
# List available test cases
$ cargo run -- -l
```

Timing results will be written to `results.csv`.

## Profiling

[samply](https://github.com/mstange/samply/tree/main) is a nice tool to use to generate profiles on MacOS. It records a profile and then, once the program has completed, it opens the profile in the Firefox profiling window so that you can inspect the results.

`samply` can be installed with `cargo`:

```sh
$ cargo install samply
```

On MacOS only programs that have been compiled on your machine can be profiled. Therefore, when you record a profile you will have to first build the executable and then run it directly:

```sh
$ cargo build
$ samply record ./target/debug/slang-timings -- WeightedPool
```

I recommend profiling a single test case for the cleanest results.
