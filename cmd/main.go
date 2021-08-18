package main

import (
	"fmt"

	"github.com/jauhararifin/kutilang/core"
	"github.com/jauhararifin/kutilang/http"
	"github.com/jauhararifin/kutilang/sqlite"
)

func main() {
	storage, err := sqlite.NewDB()
	if err != nil {
		panic(fmt.Errorf("cannot initialize db: %w", err))
	}

	controller := core.NewController(storage)
	server := http.NewHandler(controller)

	fmt.Println("starting http server at port 5000")
	if err := server.Start(":5000"); err != nil {
		panic(fmt.Errorf("cannot start http: %w", err))
	}
}
