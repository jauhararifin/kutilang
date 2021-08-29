#!/bin/bash

./scripts/buildweb.sh
mkdir -p bin
go build -o bin/kutilang cmd/*.go

CXX_FOR_TARGET=i686-w64-mingw32-g++ CC=x86_64-w64-mingw32-gcc CGO_ENABLED=1 GOOS=windows GOARCH=amd64 go build \
	-o bin/kutilang_amd64.exe \
	-ldflags -H=windowsgui \
	cmd/main.go

