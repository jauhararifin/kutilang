package sqlite

import (
	"fmt"
	"os"
	"path"
)

const appName = "kutilang"

// TODO (jauhararifin): add debug path for the sqlite.
func PrepareSqliteFile() (string, error) {
	configDir, err := os.UserConfigDir()
	if err != nil {
		return "", fmt.Errorf("cannot find config dir: %w", err)
	}

	if err := os.Mkdir(path.Join(configDir, appName), 0755); err != nil && !os.IsExist(err) {
		return "", fmt.Errorf("cannot create app dir: %w", err)
	}

	return path.Join(configDir, appName, "db.sqlite3"), nil
}
