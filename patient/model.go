package patient

import (
	"context"
	"io"
)

type Profile struct {
	ID             string `json:"id"`
	Name           string `json:"name"`
	HeadFamilyName string `json:"headFamilyName"`
	Birthdate      string `json:"birthDate"`
	Address        string `json:"address"`
	LastVisit      string `json:"lastVisit"`
}

type SearchResult struct {
	Results    []Profile `json:"results"`
	NextCursor string    `json:"nextCursor"`
}

type SearchOption struct {
	Cursor    string
	BatchSize int
}

type Visit struct {
	ID        string `json:"id"`
	PatientID string `json:"patientId"`
	Date      string `json:"date"`
	Diagnosis string `json:"diagnosis"`
	IsDeleted bool   `json:"isDeleted"`
}

type VisitQueryOption struct {
	BatchSize  int
	NextCursor string
}

type VisitQueryResult struct {
	Results    []Visit `json:"results"`
	NextCursor string  `json:"nextCursor"`
}

type Service interface {
	InsertPatient(ctx context.Context, patient Profile) (Profile, error)
	GetPatient(ctx context.Context, patientID string) (Profile, error)
	SearchPatient(ctx context.Context, query string, option SearchOption) (SearchResult, error)
	UpdatePatient(ctx context.Context, patient Profile) error
	DeletePatient(ctx context.Context, patientID string) error

	InsertPatientVisit(ctx context.Context, patientVisit Visit) (Visit, error)
	GetPatientVisits(ctx context.Context, patientID string, option VisitQueryOption) (VisitQueryResult, error)
	DeletePatientVisit(ctx context.Context, visitID string) error
	UpdatePatientVisit(ctx context.Context, patientVisit Visit) (Visit, error)
}

type BackupService interface {
	BackupPatients(ctx context.Context, w io.Writer) error
	RestorePatients(ctx context.Context, r io.Reader) error
	BackupVisits(ctx context.Context, w io.Writer) error
	RestoreVisits(ctx context.Context, r io.Reader) error

	BackupAll(ctx context.Context, w io.Writer) error
	RestoreAll(ctx context.Context, r io.Reader) error
}
