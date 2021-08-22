package sqlite

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"strconv"
	"strings"
	"sync"
	"time"

	_ "github.com/mattn/go-sqlite3"

	"github.com/jauhararifin/kutilang/log"
	"github.com/jauhararifin/kutilang/patient"
)

// TODO (jauhararifin): make prepare statement once.
type db struct {
	logger      log.Log
	conn        *sql.DB
	m           *sync.Mutex
	initialized bool
}

func NewDB(logger log.Log) (*db, error) {
	sqlitePath, err := PrepareSqliteFile()
	if err != nil {
		return nil, fmt.Errorf("cannot prepare sqlite file: %w", err)
	}

	conn, err := sql.Open("sqlite3", sqlitePath)
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

	tx, err := d.conn.Begin()
	if err != nil {
		return fmt.Errorf("cannot begin transaction: %w", err)
	}
	defer tx.Rollback()

	stmt, err := tx.PrepareContext(ctx, insertPatientSQL)
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
		false,
	); err != nil {
		return fmt.Errorf("cannot execute insert statement: %w", err)
	}

	indexStmt, err := tx.PrepareContext(ctx, insertIndexSQL)
	if err != nil {
		return fmt.Errorf("cannot prepare insert index statement: %w", err)
	}
	defer indexStmt.Close()

	for _, keyword := range keywords {
		if _, err := indexStmt.ExecContext(ctx, keyword, patient.LastVisit.Unix(), patient.ID); err != nil {
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

func (d *db) GetPatient(ctx context.Context, patientID string) (patient.Profile, error) {
	if err := d.init(ctx); err != nil {
		return patient.Profile{}, fmt.Errorf("cannot init table: %w", err)
	}

	row := d.conn.QueryRowContext(ctx, getPatientSQL, patientID)
	if row.Err() != nil {
		return patient.Profile{}, fmt.Errorf("cannot query patients table: %w", row.Err())
	}

	result, err := d.scanPatient(row)
	if err != nil {
		return patient.Profile{}, fmt.Errorf("cannot scan patient from db: %w", err)
	}

	return result, nil
}

type scanner interface {
	Scan(dest ...interface{}) error
}

func (d *db) scanPatient(row scanner) (patient.Profile, error) {
	result := patient.Profile{}
	lastVisitUnix := int64(0)
	if err := row.Scan(
		&result.ID,
		&result.Name,
		&result.HeadFamilyName,
		&result.Birthdate,
		&result.Address,
		&lastVisitUnix,
	); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return patient.Profile{}, fmt.Errorf("patient does not exist")
		}
		return patient.Profile{}, err
	}
	result.LastVisit = time.Unix(lastVisitUnix, 0)

	return result, nil
}

func (d *db) SearchPatient(
	ctx context.Context,
	keywords []string,
	lastVisit time.Time,
	lastPatientID string,
	batchSize int,
) ([]patient.Profile, error) {
	if len(keywords) == 0 {
		return nil, fmt.Errorf("empty keyword")
	}
	if err := d.init(ctx); err != nil {
		return nil, fmt.Errorf("cannot init table: %w", err)
	}
	lastVisitTs := strconv.FormatInt(lastVisit.Unix(), 10)

	sql := fmt.Sprintf(searchSQL, "(?"+strings.Repeat(",?", len(keywords)-1)+")")
	args := make([]interface{}, 0, len(keywords)+4)
	args = append(args, lastVisitTs, lastVisitTs, lastPatientID)
	for _, keyword := range keywords {
		args = append(args, keyword)
	}
	args = append(args, batchSize)
	rows, err := d.conn.QueryContext(ctx, sql, args...)
	if err != nil {
		return nil, fmt.Errorf("cannot query search sql: %w", err)
	}
	defer rows.Close()

	patients := make([]patient.Profile, 0)
	for rows.Next() {
		pat, err := d.scanPatient(rows)
		if err != nil {
			return nil, fmt.Errorf("cannot scan patient from db: %w", err)
		}
		patients = append(patients, pat)
	}

	return patients, nil
}

func (d *db) UpdatePatient(ctx context.Context, patient patient.Profile, keywords []string) error {
	if err := d.init(ctx); err != nil {
		return fmt.Errorf("cannot init table: %w", err)
	}

	tx, err := d.conn.Begin()
	if err != nil {
		return fmt.Errorf("cannot begin transaction: %w", err)
	}
	defer tx.Rollback()

	updateStmt, err := tx.PrepareContext(ctx, updatePatientSQL)
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

	indexDeleteStmt, err := tx.PrepareContext(ctx, deleteIndexSQL)
	if err != nil {
		return fmt.Errorf("cannot prepare update index statement: %w", err)
	}
	defer indexDeleteStmt.Close()

	if _, err := indexDeleteStmt.ExecContext(ctx, patient.ID); err != nil {
		return fmt.Errorf("cannot execute delete statement: %w", err)
	}

	indexStmt, err := tx.PrepareContext(ctx, insertIndexSQL)
	if err != nil {
		return fmt.Errorf("cannot prepare update index statement: %w", err)
	}
	defer indexStmt.Close()

	for _, keyword := range keywords {
		if _, err := indexStmt.ExecContext(ctx, keyword, patient.LastVisit.Unix(), patient.ID); err != nil {
			return fmt.Errorf("cannot insert keyword %s for patient: %w", keyword, err)
		}
	}

	if err := tx.Commit(); err != nil {
		return fmt.Errorf("cannot commit transaction: %w", err)
	}
	return nil
}

func (d *db) DeletePatient(ctx context.Context, patientID string) error {
	if err := d.init(ctx); err != nil {
		return fmt.Errorf("cannot init table: %w", err)
	}

	tx, err := d.conn.Begin()
	if err != nil {
		return fmt.Errorf("cannot begin transaction: %w", err)
	}
	defer tx.Rollback()

	deleteStmt, err := tx.PrepareContext(ctx, deletePatientSQL)
	if err != nil {
		return fmt.Errorf("cannot prepare delete statement: %w", err)
	}
	defer deleteStmt.Close()

	updateRes, err := deleteStmt.ExecContext(ctx, patientID)
	if err != nil {
		return fmt.Errorf("cannot execute update statement: %w", err)
	}

	n, err := updateRes.RowsAffected()
	if err != nil {
		return fmt.Errorf("cannot check number of rows affected: %w", err)
	}
	if n < 1 {
		return fmt.Errorf("patient does not exist")
	}

	indexDeleteStmt, err := tx.PrepareContext(ctx, deleteIndexSQL)
	if err != nil {
		return fmt.Errorf("cannot prepare update index statement: %w", err)
	}
	defer indexDeleteStmt.Close()

	if _, err := indexDeleteStmt.ExecContext(ctx, patientID); err != nil {
		return fmt.Errorf("cannot execute delete statement: %w", err)
	}

	if err := tx.Commit(); err != nil {
		return fmt.Errorf("cannot commit transaction: %w", err)
	}

	return nil
}

func (d *db) InsertPatientVisit(ctx context.Context, patientVisit patient.Visit) error {
	if err := d.init(ctx); err != nil {
		return fmt.Errorf("cannot init table: %w", err)
	}

	tx, err := d.conn.Begin()
	if err != nil {
		return fmt.Errorf("cannot create transaction: %w", err)
	}
	defer tx.Rollback()

	row := tx.QueryRowContext(ctx, getPatientSQL, patientVisit.PatientID)
	if _, err := d.scanPatient(row); err != nil {
		return fmt.Errorf("error scanning row: %w", err)
	}

	if _, err := tx.ExecContext(
		ctx,
		insertVisitSQL,
		patientVisit.ID,
		patientVisit.PatientID,
		patientVisit.Date.Unix(),
		patientVisit.Diagnosis,
		false,
	); err != nil {
		return fmt.Errorf("cannot exec insert sql: %w", err)
	}

	if _, err := tx.ExecContext(
		ctx,
		updatePatientLastVisitSQL,
		patientVisit.Date.Unix(),
		patientVisit.PatientID,
	); err != nil {
		return fmt.Errorf("cannot exec update patient sql: %w", err)
	}

	if err := tx.Commit(); err != nil {
		return fmt.Errorf("error commiting transaction: %w", err)
	}

	return nil
}

func (d *db) GetPatientVisits(
	ctx context.Context,
	patientID string,
	dateCursor time.Time,
	idCursor string,
	batchSize int,
) ([]patient.Visit, error) {
	if err := d.init(ctx); err != nil {
		return nil, fmt.Errorf("cannot init table: %w", err)
	}
	dateCursorTs := strconv.FormatInt(dateCursor.Unix(), 10)

	rows, err := d.conn.QueryContext(ctx, getVisitSQL, patientID, dateCursorTs, dateCursorTs, idCursor, batchSize)
	if err != nil {
		return nil, fmt.Errorf("cannot query search sql: %w", err)
	}
	defer rows.Close()

	patientVisits := make([]patient.Visit, 0)
	for rows.Next() {
		patientVisit, err := d.scanPatientVisit(rows)
		if err != nil {
			return nil, fmt.Errorf("cannot scan patient visit from db: %w", err)
		}
		patientVisits = append(patientVisits, patientVisit)
	}

	return patientVisits, nil
}

func (d *db) scanPatientVisit(row scanner) (patient.Visit, error) {
	result := patient.Visit{}
	timestamp := int64(0)
	if err := row.Scan(
		&result.ID,
		&result.PatientID,
		&timestamp,
		&result.Diagnosis,
		&result.IsDeleted,
	); err != nil {
		return patient.Visit{}, err
	}
	result.Date = time.Unix(int64(timestamp), 0)

	return result, nil
}

func (d *db) DeletePatientVisit(ctx context.Context, visitID string) error {
	if err := d.init(ctx); err != nil {
		return fmt.Errorf("cannot init table: %w", err)
	}

	if _, err := d.conn.ExecContext(ctx, deleteVisitSQL, visitID); err != nil {
		return fmt.Errorf("cannot exec update sql: %w", err)
	}

	return nil
}

func (d *db) UpdatePatientVisit(ctx context.Context, patientVisit patient.Visit) error {
	if err := d.init(ctx); err != nil {
		return fmt.Errorf("cannot init table: %w", err)
	}

	if _, err := d.conn.ExecContext(
		ctx,
		updateVisitSQL,
		patientVisit.Diagnosis,
		patientVisit.ID,
		false,
	); err != nil {
		return fmt.Errorf("cannot exec update sql: %w", err)
	}

	return nil
}
