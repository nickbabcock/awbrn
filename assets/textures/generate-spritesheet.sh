#!/bin/bash

set -euo pipefail

DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
cd "$DIR"

(cd "$DIR/../.." && cargo xtask-assets tiles)
(cd "$DIR/../.." && cargo xtask-assets units)
