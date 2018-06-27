# Holochain in Rust
This is a beginning and architecture proposal of a second iteration of
Holochain build in Rust with the intention to have
1. some modules be compiled to WebAssembly to be reused in Holo's front-end part and
2. be able to use a somewhat mature WebAssembly interperter like [wasmi](https://github.com/paritytech/wasmi) for a new type of Ribosome.

## Build/install
First [install rustup](https://www.rust-lang.org/en-US/install.html).

We are pinning the rust version to ensure predictable behaviour.

To install this specific version and set it as the default.

```
rustup install 1.26.2
rustup default 1.26.2
```

and then just run

```
cargo build
```

inside this repository.
Find the executable in ```target/debug/holochain-beta```.

To run the tests (which are in ```src/lib.rs```) just say

```
cargo test
```

Note that some lints/warnings will only appear on a cold cargo run, which is
slower but represents what travis will see during CI.

To run all cargo tests from a cold start:

```
cargo clean && cargo test --verbose --all
```

### Building for Android
Holochain can be build for Android as described [here](https://mozilla.github.io/firefox-browser-architecture/experiments/2017-09-21-rust-on-android.html).
In order to get to libraries that can be linked against when building [HoloSqape](https://github.com/holochain/holosqape)
for Android, you basically just need to setup up according targets for cargo.

Given that the Android SDK is installed, you first need to create standalone NDKs like so:

```bash
export ANDROID_HOME=/Users/$USER/Library/Android/sdk
export NDK_HOME=$ANDROID_HOME/ndk-bundle
cd ~
mkdir NDK
${NDK_HOME}/build/tools/make_standalone_toolchain.py --api 26 --arch arm64 --install-dir NDK/arm64
${NDK_HOME}/build/tools/make_standalone_toolchain.py --api 26 --arch arm --install-dir NDK/arm
${NDK_HOME}/build/tools/make_standalone_toolchain.py --api 26 --arch x86 --install-dir NDK/x86
```

Then add the following lines to your ```~/.cargo/config```:

```toml
[target.aarch64-linux-android]
ar = "~/NDK/arm64/bin/aarch64-linux-android-ar"
linker = "~/greetings/NDK/arm64/bin/aarch64-linux-android-clang"

[target.armv7-linux-androideabi]
ar = "~/NDK/arm/bin/arm-linux-androideabi-ar"
linker = "~/NDK/arm/bin/arm-linux-androideabi-clang"

[target.i686-linux-android]
ar = "~/NDK/x86/bin/i686-linux-android-ar"
linker = "~/NDK/x86/bin/i686-linux-android-clang"

```

Now you can add those targets to your rust installation with:

```
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android
```

You should be able to build Holochain for Android with:

```
cd <holochain repo>
cargo build --target armv7-linux-androideabi --release
```
for instance.

## Architecture
I've tried to resemble Redux in Rust and looked at [this code](https://github.com/rust-redux/rust-redux).

instance::Instance has a state::State which is the one global state with
sub-state slices for each module which are defined in each module respectively
(see src/agent/mod.rs, src/network/mod.rs and src/nucleus/mod.rs) and put
together in src/state.rs.

State is only read from the instance

```rs
instance.state().nucleus().dna()
```

and mutated by dispatching an action:

```rs
let entry = Entry{...};
instance.dispatch(state::Action::Agent(Commit(entry)));
```

Instance calls reduce on the state with the next action to consume:

```rs
pub fn consume_next_action(&mut self) {
    if self.pending_actions.len() > 0 {
        let action = self.pending_actions.pop_front().unwrap();
        self.state = self.state.clone().reduce(&action);
    }
}
```

The main reducer creates a new State object and calls the sub-reducers:

```rs
pub fn reduce(&mut self, action: &Action) -> Self {
    State {
        nucleus: ::nucleus::reduce(Rc::clone(&self.nucleus), action),
        agent: ::agent::reduce(Rc::clone(&self.agent), action)

    }
}
```

The module 'state' defines an action type (enum state::Action) that has values for
each sub-module. The modules define their sub-actions themselves and provide
their own sub-reducer function that handles those action types.

Since sub-module state slices are included in state::State as counted references (Rc\<AgentState>) the sub-module reducers can choose if they have the new state object (that the reducer returns) reference the same old sub-state slice (when the action did not affect the sub-state for instance) or if they clone the state, mutate it and return a different reference.

In module agent:

```rs
pub fn reduce(old_state: Rc<AgentState>, action: &_Action) -> Rc<AgentState> {
    match *action {
        _Action::Agent(ref agent_action) => {
            let mut new_state: AgentState = (*old_state).clone();
            match *agent_action {
                Action::Commit(ref entry) => {

                }
            }
            Rc::new(new_state)
        },
        _ => old_state
    }
}
```

With every module handling its state which is read-only for everything else and providing actions to be created from anywhere else that are processed through the reducer hierarchy I hope to decouple modules effectively. Actions being logged make already for a great debugging tool, if that is not enough, the state history could be stored and in a future debugging tool even switched back and forth (time-machine debugging for Holochain :D).

## Local development & testing

CI builds are happening on circle CI.

### Docker

The `docker` folder contains scripts to build and run docker images.

#### Standard build

Build:

`. docker/build-amd64`

Run:

`. docker/run`

#### Code coverage

Build:

`. docker/build-codecov`

Run:

`. docker/run-codecov`

#### Code style

There is a linter enforcing code style.

Build:

```
. docker/build-lint
```

Run:

`. docker/run-lint`

### Watch tests

For better productivity, watch your cargo tests/check while you work.

Install:

`cargo install cargo-watch`

Run:

```
cargo watch # check
cargo watch -x test # test
```

## License
[![License: GPL v3](https://img.shields.io/badge/License-GPL%20v3-blue.svg)](http://www.gnu.org/licenses
/gpl-3.0)

Copyright (C) 2018, Holochain Trust

This program is free software: you can redistribute it and/or modify it under the terms of the license p
rovided in the LICENSE file (GPLv3).  This program is distributed in the hope that it will be useful, bu
t WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR
 PURPOSE.

**Note:** We are considering other 'looser' licensing options (like MIT license) but at this stage are u
sing GPL while we're getting the matter sorted out.
