package core

import (
	"context"
	"encoding/csv"
	"io"

	"github.com/jauhararifin/kutilang/log"
)

type backupController struct {
	storage Storage
	logger  log.Log
}

func (c *backupController) BackupPatients(ctx context.Context, w io.Writer) error {
	patients, err := c.storage.GetAllPatients(ctx)
	if err != nil {
		return err
	}

	csvWriter := csv.NewWriter(w)
	for patient := range patients {
		if err := csvWriter.Write([]string{
			patient.ID,
			patient.Name,
			patient.HeadFamilyName,
			patient.Birthdate,
			patient.Address,
			patient.LastVisit,
		}); err != nil {
			return err
		}
	}
	csvWriter.Flush()

	return csvWriter.Error()
}

func NewBackupController(storage Storage, logger log.Log) *backupController {
	return &backupController{storage: storage, logger: logger}
}
