package main

import (
	"context"
	"fmt"
	"io"
	"os"
	"path"

	"github.com/sirupsen/logrus"
	"github.com/webview/webview"
	"gopkg.in/natefinch/lumberjack.v2"

	"github.com/jauhararifin/kutilang/core"
	"github.com/jauhararifin/kutilang/http"
	"github.com/jauhararifin/kutilang/log"
	"github.com/jauhararifin/kutilang/sqlite"
)

const appName = "Kutilang"
const port = 5000

func main() {
	logWriter, err := prepareLogFile()
	if err != nil {
		fmt.Println("cannot use default log file")
		logWriter = os.Stdout
	}
	defer logWriter.Close()
	logrusLogger := logrus.New()
	logrusLogger.SetFormatter(&logrus.JSONFormatter{})
	logrusLogger.SetOutput(logWriter)
	logger := log.New(logrusLogger)

	dbFile, err := prepareDBFile()
	if err != nil {
		panic(fmt.Errorf("cannot initialize db file: %w", err))
	}

	storage, err := sqlite.NewDB(dbFile, logger)
	if err != nil {
		panic(fmt.Errorf("cannot initialize db: %w", err))
	}

	controller := core.NewController(storage, logger)
	server := http.NewHandler(controller, logger)

	go func() {
		logger.Log(context.Background(), "starting http server", "port", port)
		if err := server.Start(fmt.Sprintf("localhost:%d", port)); err != nil {
			panic(fmt.Errorf("cannot start http: %w", err))
		}
	}()

	w := webview.New(false)
	defer w.Destroy()
	w.SetTitle(appName)
	w.SetSize(800, 600, webview.HintNone)
	w.Navigate(fmt.Sprintf("http://localhost:%d/", port))
	w.Run()
}

func prepareLogFile() (io.WriteCloser, error) {
	appDir, err := prepareAppDir()
	if err != nil {
		return nil, fmt.Errorf("cannot prepare app dir: %w", err)
	}

	logFile := path.Join(appDir, "log.log")
	return &lumberjack.Logger{
		Filename:   logFile,
		MaxSize:    1024, // megabytes
		MaxBackups: 3,
		MaxAge:     28, //days
	}, nil
}

func prepareAppDir() (string, error) {
	configDir, err := os.UserConfigDir()
	if err != nil {
		return "", fmt.Errorf("cannot find config dir: %w", err)
	}

	appDir := path.Join(configDir, appName)
	if err := os.Mkdir(appDir, 0755); err != nil && !os.IsExist(err) {
		return "", fmt.Errorf("cannot create app dir: %w", err)
	}

	return appDir, nil
}

func prepareDBFile() (string, error) {
	appDir, err := prepareAppDir()
	if err != nil {
		return "", fmt.Errorf("cannot prepare app dir: %w", err)
	}

	return path.Join(appDir, "db.sqlite3"), nil
}
