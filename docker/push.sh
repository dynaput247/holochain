#!/usr/bin/env bash

set -euxo pipefail

docker push "holochain/holochain-rust:${1}"
