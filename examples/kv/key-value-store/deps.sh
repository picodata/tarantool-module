#!/bin/sh
# Call this scripts to install key-value's dependencies

set -e

tarantoolctl rocks make ./key-value-scm-1.rockspec
