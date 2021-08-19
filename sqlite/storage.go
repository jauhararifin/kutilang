package sqlite

import (
	"context"
	"fmt"
	"sync"

	"github.com/jmoiron/sqlx"
	_ "github.com/mattn/go-sqlite3"

	"github.com/jauhararifin/kutilang/log"
	"github.com/jauhararifin/kutilang/patient"
)

// TODO (jauhararifin): make prepare statement once.
type db struct {
	logger      log.Log
	conn        *sqlx.DB
	m           *sync.Mutex
	initialized bool
}

func NewDB(logger log.Log) (*db, error) {
	sqlitePath, err := PrepareSqliteFile()
	if err != nil {
		return nil, fmt.Errorf("cannot prepare sqlite file: %w", err)
	}

	conn, err := sqlx.Connect("sqlite3", sqlitePath)
	if err != nil {
		return nil, fmt.Errorf("cannot connect to sqlite3: %w", err)
	}

	return &db{
		logger:      logger,
		conn:        conn,
		m:           &sync.Mutex{},
		initialized: false,
	}, nil
}

func (d *db) InsertPatient(ctx context.Context, patient patient.Profile, keywords []string) error {
	if err := d.init(ctx); err != nil {
		return fmt.Errorf("cannot init table: %w", err)
	}

	tx, err := d.conn.Beginx()
	if err != nil {
		return fmt.Errorf("cannot begin transaction: %w", err)
	}
	defer tx.Rollback()

	stmt, err := tx.PreparexContext(ctx, insertPatientSQL)
	if err != nil {
		return fmt.Errorf("cannot prepare insert statement: %w", err)
	}
	defer stmt.Close()

	if _, err := stmt.ExecContext(
		ctx,
		patient.ID,
		patient.Name,
		patient.HeadFamilyName,
		patient.Birthdate,
		patient.Address,
		patient.LastVisit.Unix(),
	); err != nil {
		return fmt.Errorf("cannot execute insert statement: %w", err)
	}

	indexStmt, err := tx.PreparexContext(ctx, insertIndexSQL)
	if err != nil {
		return fmt.Errorf("cannot prepare insert index statement: %w", err)
	}
	defer indexStmt.Close()

	for _, keyword := range keywords {
		if _, err := indexStmt.ExecContext(ctx, keyword, patient.LastVisit, patient.ID); err != nil {
			return fmt.Errorf("cannot insert keyword %s for patient: %w", keyword, err)
		}
	}

	if err := tx.Commit(); err != nil {
		return fmt.Errorf("cannot commit transaction: %w", err)
	}
	return nil
}

func (d *db) init(ctx context.Context) error {
	d.m.Lock()
	defer d.m.Unlock()

	if d.initialized {
		return nil
	}

	if _, err := d.conn.ExecContext(ctx, tableInitSQL); err != nil {
		return fmt.Errorf("cannot execute init table sql: %w", err)
	}

	d.initialized = true
	return nil
}

func (d *db) SearchPatient(
	ctx context.Context,
	query string,
	option patient.SearchOption,
) (patient.SearchResult, error) {
	panic("impl")
}

func (d *db) UpdatePatient(ctx context.Context, patient patient.Profile, keywords []string) error {
	if err := d.init(ctx); err != nil {
		return fmt.Errorf("cannot init table: %w", err)
	}

	tx, err := d.conn.Beginx()
	if err != nil {
		return fmt.Errorf("cannot begin transaction: %w", err)
	}
	defer tx.Rollback()

	updateStmt, err := tx.PreparexContext(ctx, updatePatientSQL)
	if err != nil {
		return fmt.Errorf("cannot prepare update statement: %w", err)
	}
	defer updateStmt.Close()

	updateRes, err := updateStmt.ExecContext(
		ctx,
		patient.Name,
		patient.HeadFamilyName,
		patient.Birthdate,
		patient.Address,
		patient.ID,
	)
	if err != nil {
		return fmt.Errorf("cannot execute update statement: %w", err)
	}

	n, err := updateRes.RowsAffected()
	if err != nil {
		return fmt.Errorf("cannot check number of rows affected: %w", err)
	}
	if n < 1 {
		return fmt.Errorf("patient is not found")
	}

	indexDeleteStmt, err := tx.PreparexContext(ctx, deleteIndexSQL)
	if err != nil {
		return fmt.Errorf("cannot prepare update index statement: %w", err)
	}
	defer indexDeleteStmt.Close()

	if _, err := indexDeleteStmt.ExecContext(ctx, patient.ID); err != nil {
		return fmt.Errorf("cannot execute delete statement: %w", err)
	}

	indexStmt, err := tx.PreparexContext(ctx, insertIndexSQL)
	if err != nil {
		return fmt.Errorf("cannot prepare update index statement: %w", err)
	}
	defer indexStmt.Close()

	for _, keyword := range keywords {
		if _, err := indexStmt.ExecContext(ctx, keyword, patient.LastVisit, patient.ID); err != nil {
			return fmt.Errorf("cannot insert keyword %s for patient: %w", keyword, err)
		}
	}

	if err := tx.Commit(); err != nil {
		return fmt.Errorf("cannot commit transaction: %w", err)
	}
	return nil
}

func (d *db) DeletePatient(ctx context.Context, patientID string) error {
	panic("impl")
}

func (d *db) GetPatientVisits(
	ctx context.Context,
	patientID string,
	option patient.VisitQueryOption,
) (patient.VisitQueryResult, error) {
	panic("impl")
}

func (d *db) InsertPatientVisit(ctx context.Context, patientVisit patient.Visit) error {
	panic("impl")
}

func (d *db) DeletePatientVisit(ctx context.Context, visitID string) error {
	panic("impl")
}

func (d *db) UpdatePatientVisit(ctx context.Context, patientVisit patient.Visit) error {
	panic("impl")
}
