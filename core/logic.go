package core

import (
	"context"
	"encoding/csv"
	"fmt"
	"io"
	"sort"
	"strings"
	"time"

	"github.com/google/uuid"

	"github.com/jauhararifin/kutilang/log"
	"github.com/jauhararifin/kutilang/patient"
)

type Storage interface {
	InsertPatient(ctx context.Context, patient patient.Profile, keywords []string) error
	GetPatient(ctx context.Context, patientID string) (patient.Profile, error)
	GetAllPatients(ctx context.Context) (<-chan patient.Profile, error)
	UpdatePatient(ctx context.Context, patient patient.Profile, keywords []string) error
	SearchPatient(
		ctx context.Context,
		keywords []string,
		lastVisit string,
		lastPatientID string,
		batchSize int,
	) ([]patient.Profile, error)
	DeletePatient(ctx context.Context, patientID string) error

	InsertPatientVisit(ctx context.Context, patientVisit patient.Visit) error
	GetPatientVisits(
		ctx context.Context,
		patientID string,
		dateCursor string,
		idCursor string,
		batchSize int,
	) ([]patient.Visit, error)
	DeletePatientVisit(ctx context.Context, visitID string) error
	UpdatePatientVisit(ctx context.Context, patientVisit patient.Visit) error
}

type controller struct {
	storage Storage
	logger  log.Log
}

func (c *controller) InsertPatient(ctx context.Context, profile patient.Profile) (patient.Profile, error) {
	if err := c.validatePatient(&profile); err != nil {
		return patient.Profile{}, fmt.Errorf("invalid patient info: %w", err)
	}

	profile.ID = c.generateID()
	profile.LastVisit = time.Now().Local().Format("2006-01-02")

	keywords := c.generateKeywords(profile)
	if err := c.storage.InsertPatient(ctx, profile, keywords); err != nil {
		return patient.Profile{}, fmt.Errorf("cannot insert patient: %w", err)
	}

	return profile, nil
}

// TODO (jauhararifin): make better validation
func (c *controller) validatePatient(patient *patient.Profile) error {
	if strings.TrimSpace(patient.Name) == "" {
		return fmt.Errorf("missing patient name")
	}
	if strings.TrimSpace(patient.Address) == "" {
		return fmt.Errorf("missing patient address")
	}
	birthDate, err := parseDate(patient.Birthdate)
	if err != nil {
		return fmt.Errorf("invalid birthdate: %w", err)
	}
	patient.Birthdate = birthDate
	return nil
}

func (c *controller) generateID() string {
	return strings.Replace(uuid.New().String(), "-", "", -1)
}

func (c *controller) generateKeywords(patient patient.Profile) []string {
	words := strings.Split(patient.Name+" "+patient.HeadFamilyName, " ")
	keywordMap := make(map[string]struct{})
	for _, word := range words {
		if len(word) == 0 {
			continue
		}
		keywordMap[strings.ToLower(word)] = struct{}{}
	}

	keywords := make([]string, 0, len(keywordMap))
	for keyword := range keywordMap {
		keywords = append(keywords, keyword)
	}

	sort.Strings(keywords)
	return keywords
}

func (c *controller) GetPatient(ctx context.Context, patientID string) (patient.Profile, error) {
	if patientID == "" {
		return patient.Profile{}, fmt.Errorf("inavlid patient id")
	}

	result, err := c.storage.GetPatient(ctx, patientID)
	if err != nil {
		return patient.Profile{}, fmt.Errorf("cannot get patient: %w", err)
	}

	return result, nil
}

func (c *controller) SearchPatient(ctx context.Context, query string, option patient.SearchOption) (patient.SearchResult, error) {
	keywords := c.getKeywordsFromQuery(query)
	if len(keywords) == 0 {
		return patient.SearchResult{}, fmt.Errorf("invalid search query")
	}
	if len(keywords) > 30 {
		return patient.SearchResult{}, fmt.Errorf("search query is too long")
	}
	if option.BatchSize > 30 {
		return patient.SearchResult{}, fmt.Errorf("batch is maximum at 30")
	}
	if option.BatchSize < 1 {
		option.BatchSize = 1
	}
	lastVisit, lastPatientID, err := c.parsePatientCursor(option.Cursor)
	if err != nil {
		return patient.SearchResult{}, fmt.Errorf("invalid cursor: %w", err)
	}

	patients, err := c.storage.SearchPatient(ctx, keywords, lastVisit, lastPatientID, option.BatchSize)
	if err != nil {
		return patient.SearchResult{}, err
	}

	result := patient.SearchResult{Results: patients}
	if len(patients) >= option.BatchSize {
		lastEntry := patients[len(patients)-1]
		result.NextCursor = c.buildPatientCursor(lastEntry.LastVisit, lastEntry.ID)
	}

	return result, nil
}

func (c *controller) getKeywordsFromQuery(query string) []string {
	keywordMap := make(map[string]struct{})
	for _, keyword := range strings.Split(strings.ToLower(query), " ") {
		keyword = strings.TrimSpace(keyword)
		if len(keyword) > 0 {
			keywordMap[keyword] = struct{}{}
		}
	}

	keywords := make([]string, 0, len(keywordMap))
	for keyword := range keywordMap {
		keywords = append(keywords, keyword)
	}

	return keywords
}

func (c *controller) parsePatientCursor(cursor string) (lastVisit string, lastaPatientID string, err error) {
	now := time.Now().Local().Format("2006-01-02")
	if cursor == "" {
		return now, "", nil
	}

	parts := strings.Split(cursor, ".")
	if len(parts) != 2 {
		return now, "", fmt.Errorf("invalid format")
	}

	return parts[0], parts[1], nil
}

func (c *controller) buildPatientCursor(lastVisit, lastPatientID string) string {
	return lastVisit + "." + lastPatientID
}

func (c *controller) UpdatePatient(ctx context.Context, patient patient.Profile) error {
	if err := c.validatePatient(&patient); err != nil {
		return fmt.Errorf("invalid patient info: %w", err)
	}

	if patient.ID == "" {
		return fmt.Errorf("missing patient id")
	}

	keywords := c.generateKeywords(patient)
	if err := c.storage.UpdatePatient(ctx, patient, keywords); err != nil {
		return fmt.Errorf("cannot update patient: %w", err)
	}

	return nil
}

func (c *controller) DeletePatient(ctx context.Context, patientID string) error {
	if patientID == "" {
		return fmt.Errorf("missing patient id")
	}
	if err := c.storage.DeletePatient(ctx, patientID); err != nil {
		return fmt.Errorf("cannot delete patient: %w", err)
	}
	return nil
}

func (c *controller) InsertPatientVisit(ctx context.Context, patientVisit patient.Visit) (patient.Visit, error) {
	if err := c.validatePatientVisit(&patientVisit); err != nil {
		return patient.Visit{}, fmt.Errorf("invalid patient visit info: %w", err)
	}

	patientVisit.ID = c.generateID()
	if patientVisit.Date == "" {
		patientVisit.Date = time.Now().Local().Format("2006-01-02")
	}

	if err := c.storage.InsertPatientVisit(ctx, patientVisit); err != nil {
		return patient.Visit{}, fmt.Errorf("cannot insert patient visit: %w", err)
	}

	return patientVisit, nil
}

func (c *controller) validatePatientVisit(patientVisit *patient.Visit) error {
	if strings.TrimSpace(patientVisit.Diagnosis) == "" {
		return fmt.Errorf("missing diagnosis")
	}
	if patientVisit.Date != "" {
		date, err := parseDate(patientVisit.Date)
		if err != nil {
			return err
		}
		patientVisit.Date = date
	}
	return nil
}

func (c *controller) GetPatientVisits(ctx context.Context, patientID string, option patient.VisitQueryOption) (patient.VisitQueryResult, error) {
	if option.BatchSize > 30 {
		return patient.VisitQueryResult{}, fmt.Errorf("batch is maximum at 30")
	}
	if option.BatchSize < 1 {
		option.BatchSize = 1
	}
	tsCursor, patientVisitIDCursor, err := c.parsePatientVisitCursor(option.NextCursor)
	if err != nil {
		return patient.VisitQueryResult{}, fmt.Errorf("invalid cursor: %w", err)
	}

	patientVisits, err := c.storage.GetPatientVisits(ctx, patientID, tsCursor, patientVisitIDCursor, option.BatchSize)
	if err != nil {
		return patient.VisitQueryResult{}, err
	}

	result := patient.VisitQueryResult{Results: patientVisits}
	if len(patientVisits) >= option.BatchSize {
		lastEntry := patientVisits[len(patientVisits)-1]
		result.NextCursor = c.buildPatientVisitCursor(lastEntry.Date, lastEntry.ID)
	}

	return result, nil
}

func (c *controller) parsePatientVisitCursor(cursor string) (lastVisit string, id string, err error) {
	now := time.Now().Local().Format("2006-01-02")
	if cursor == "" {
		return now, "", nil
	}

	parts := strings.Split(cursor, ".")
	if len(parts) != 2 {
		return now, "", fmt.Errorf("invalid format")
	}

	return parts[0], parts[1], nil
}

func (c *controller) buildPatientVisitCursor(lastVisit string, id string) string {
	return lastVisit + "." + id
}

func (c *controller) DeletePatientVisit(ctx context.Context, visitID string) error {
	if visitID == "" {
		return fmt.Errorf("missing patient visit id")
	}
	if err := c.storage.DeletePatientVisit(ctx, visitID); err != nil {
		return fmt.Errorf("cannot delete patient visit: %w", err)
	}
	return nil
}

func (c *controller) UpdatePatientVisit(ctx context.Context, patientVisit patient.Visit) (patient.Visit, error) {
	if err := c.validatePatientVisit(&patientVisit); err != nil {
		return patient.Visit{}, fmt.Errorf("invalid patient visit info: %w", err)
	}

	if patientVisit.ID == "" {
		return patient.Visit{}, fmt.Errorf("missing patient id visit")
	}

	if patientVisit.Date == "" {
		patientVisit.Date = time.Now().Local().Format("2006-01-02")
	}

	if err := c.storage.UpdatePatientVisit(ctx, patientVisit); err != nil {
		return patient.Visit{}, fmt.Errorf("cannot update patient visit: %w", err)
	}

	return patientVisit, nil
}

func NewController(storage Storage, logger log.Log) *controller {
	return &controller{storage: storage, logger: logger}
}
