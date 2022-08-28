#!/bin/bash

./scripts/buildweb.sh

go mod vendor

mkdir -p bin
go build -mod=vendor -o bin/linux/kutilang cmd/*.go
