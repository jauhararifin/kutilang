#!/bin/bash

./scripts/buildweb.sh

go mod vendor

mkdir -p bin/linux
go build -mod=vendor -o bin/linux/kutilang cmd/*.go

mkdir -p bin/windows
CC=x86_64-w64-mingw32-gcc GOOS=windows CGO_ENABLED=1 go build -mod=vendor -o bin/kutilang.exe cmd/*.go
