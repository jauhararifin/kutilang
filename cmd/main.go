package main

import (
	"fmt"

	"github.com/sirupsen/logrus"

	"github.com/jauhararifin/kutilang/core"
	"github.com/jauhararifin/kutilang/http"
	"github.com/jauhararifin/kutilang/log"
	"github.com/jauhararifin/kutilang/sqlite"
)

func main() {
	logrusLogger := logrus.New()
	logrusLogger.SetFormatter(&logrus.JSONFormatter{})
	logger := log.New(logrusLogger)

	storage, err := sqlite.NewDB(logger)
	if err != nil {
		panic(fmt.Errorf("cannot initialize db: %w", err))
	}

	controller := core.NewController(storage, logger)
	server := http.NewHandler(controller, logger)

	fmt.Println("starting http server at port 5000")
	if err := server.Start(":5000"); err != nil {
		panic(fmt.Errorf("cannot start http: %w", err))
	}
}
