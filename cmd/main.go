package main

import (
	"context"
	"fmt"
	"io"
	"os"
	"path"

	"github.com/getlantern/systray"
	"github.com/sirupsen/logrus"
	"github.com/spf13/cobra"
	"gopkg.in/natefinch/lumberjack.v2"

	"github.com/jauhararifin/kutilang/core"
	"github.com/jauhararifin/kutilang/http"
	"github.com/jauhararifin/kutilang/log"
	"github.com/jauhararifin/kutilang/sqlite"
)

const appName = "kutilang"

var (
	dbFlag   string
	addrFlag string
)

var rootCmd = &cobra.Command{
	Use:   "kutilang",
	Short: "Kuting is a simple clinic management app",
	Run:   runServer,
}

func init() {
	rootCmd.Flags().StringVar(&dbFlag, "db", "", "path to the sqlite3 database file (default: <user config dir>/kutilang/db.sqlite3)")
	rootCmd.Flags().StringVar(&addrFlag, "addr", ":5000", "address for the http server to listen on")
}

func main() {
	if err := rootCmd.Execute(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func runServer(cmd *cobra.Command, args []string) {
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

	dbFile := dbFlag
	if dbFile == "" {
		dbFile, err = prepareDBFile()
		if err != nil {
			panic(fmt.Errorf("cannot initialize db file: %w", err))
		}
	}

	storage, err := sqlite.NewDB(dbFile, logger)
	if err != nil {
		panic(fmt.Errorf("cannot initialize db: %w", err))
	}

	controller := core.NewController(storage, logger)
	backupController := core.NewBackupController(storage, logger)
	server := http.NewHandler(controller, backupController, logger)

	ctx := context.Background()
	trayReady := make(chan struct{})

	// The http server runs in the background. If it stops on its own (e.g. the
	// port is already in use), the whole app quits, same as before.
	go func() {
		logger.Log(ctx, "starting http server", "addr", addrFlag, "db", dbFile)
		if err := server.Start(addrFlag); err != nil {
			logger.Log(ctx, "server stopped", "err", err)
		}
		<-trayReady
		systray.Quit()
	}()

	// The tray must run on the main thread (required by macOS, harmless on
	// Windows). systray locks the main goroutine to the main thread in its
	// init. It blocks until "Quit" is clicked.
	systray.Run(func() {
		quit := systray.AddMenuItem("Quit", "Quit the whole app")
		systray.SetTitle("Klinik Kutilang")
		close(trayReady)
		go func() {
			<-quit.ClickedCh
			systray.Quit()
		}()
	}, func() {
		server.Stop()
	})
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
