package patient

import (
	"context"
	"time"
)

type Profile struct {
	ID             string    `json:"id"`
	Name           string    `json:"name"`
	Birthdate      string    `json:"birthDate"`
	HeadFamilyName string    `json:"headFamilyName"`
	Address        string    `json:"address"`
	LastVisit      time.Time `json:"lastVisit"`
}

type SearchResult struct {
	Results    []Profile `json:"results"`
	NextCursor string    `json:"nextCursor"`
}

type SearchOption struct {
	BatchSize int
}

type Visit struct {
	ID        string    `json:"id"`
	PatientID string    `json:"patientId"`
	Date      time.Time `json:"date"`
	Diagnosis string    `json:"diagnosis"`
	IsDeleted bool      `json:"isDeleted"`
}

type VisitQueryOption struct {
	BatchSize int
}

type VisitQueryResult struct {
	Results    []Visit `json:"results"`
	NextCursor string  `json:"nextCursor"`
}

type Service interface {
	InsertPatient(ctx context.Context, patient Profile) (Profile, error)
	SearchPatient(ctx context.Context, query string, option SearchOption) (SearchResult, error)
	UpdatePatient(ctx context.Context, patient Profile) error
	DeletePatient(ctx context.Context, patientID string) error

	InsertPatientVisit(ctx context.Context, patientVisit Visit) error
	GetPatientVisits(ctx context.Context, patientID string, option VisitQueryOption) (VisitQueryResult, error)
	DeletePatientVisit(ctx context.Context, visitID string) error
	UpdatePatientVisit(ctx context.Context, patientVisit Visit) error
}
