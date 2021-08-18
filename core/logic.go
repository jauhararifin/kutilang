package core

import (
	"context"
	"fmt"
	"sort"
	"strings"
	"time"

	"github.com/google/uuid"

	"github.com/jauhararifin/kutilang/patient"
)

type Storage interface {
	InsertPatient(ctx context.Context, patient patient.Profile, keywords []string) error
}

type controller struct {
	storage Storage
}

func (c *controller) InsertPatient(ctx context.Context, profile patient.Profile) (patient.Profile, error) {
	if err := c.validatePatient(profile); err != nil {
		return patient.Profile{}, fmt.Errorf("invalid patient info: %w", err)
	}

	profile.ID = c.generateID()
	profile.LastVisit = time.Now()
	keywords := c.generateKeywords(profile)
	if err := c.storage.InsertPatient(ctx, profile, keywords); err != nil {
		return patient.Profile{}, fmt.Errorf("cannot insert patient: %w", err)
	}

	profile.LastVisit = profile.LastVisit.Local()
	profile.Birthdate = profile.Birthdate.Local()
	return profile, nil
}

// TODO (jauhararifin): make better validation
func (c *controller) validatePatient(patient patient.Profile) error {
	if strings.TrimSpace(patient.Name) == "" {
		return fmt.Errorf("missing patient name")
	}
	if strings.TrimSpace(patient.Address) == "" {
		return fmt.Errorf("missing patient address")
	}
	return nil
}

func (c *controller) generateID() string {
	return strings.Replace(uuid.New().String(), "-", "", -1)
}

func (c *controller) generateKeywords(patient patient.Profile) []string {
	words := strings.Split(patient.Name, " ")
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

func (c *controller) SearchPatient(ctx context.Context, query string, option patient.SearchOption) (patient.SearchResult, error) {
	panic("impl")
}

func (c *controller) UpdatePatient(ctx context.Context, patient patient.Profile) error {
	panic("impl")
}

func (c *controller) DeletePatient(ctx context.Context, patientID string) error {
	panic("impl")
}

func (c *controller) GetPatientVisits(ctx context.Context, patientID string, option patient.VisitQueryOption) (patient.VisitQueryResult, error) {
	panic("impl")
}

func (c *controller) InsertPatientVisit(ctx context.Context, patientVisit patient.Visit) error {
	panic("impl")
}

func (c *controller) DeletePatientVisit(ctx context.Context, visitID string) error {
	panic("impl")
}

func (c *controller) UpdatePatientVisit(ctx context.Context, patientVisit patient.Visit) error {
	panic("impl")
}

func NewController(storage Storage) *controller {
	return &controller{storage: storage}
}
