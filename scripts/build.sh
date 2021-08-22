#!/bin/bash

./scripts/buildweb.sh
mkdir -p bin
go build -o bin/kutilang cmd/*.go

