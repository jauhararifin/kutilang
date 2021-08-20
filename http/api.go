package http

import (
	"encoding/json"
	"fmt"
	"net/http"

	"github.com/gorilla/mux"

	"github.com/jauhararifin/kutilang/log"
	"github.com/jauhararifin/kutilang/patient"
)

type handler struct {
	service patient.Service
	logger  log.Log
}

func (h *handler) Start(addr string) error {
	router := mux.NewRouter()
	router.Path("/api/v1/patients/").HandlerFunc(h.handleInsertPatient).Methods("POST")
	router.Path("/api/v1/search/").HandlerFunc(h.handleSearchPatient).Methods("GET")//.Queries("q", "{q}", "cursor", "{cursor}")
	router.Path("/api/v1/patients/{patientID}").HandlerFunc(h.handleGetPatient).Methods("GET")
	router.Path("/api/v1/patients/{patientID}").HandlerFunc(h.handleUpdatePatient).Methods("PUT")
	router.PathPrefix("/").Handler(staticFileHandler()).Methods("GET")

	router.Use(log.NewMiddleware(h.logger))

	return http.ListenAndServe(addr, router)
}

func (h *handler) handleInsertPatient(w http.ResponseWriter, req *http.Request) {
	patientProfile := patient.Profile{}
	if err := json.NewDecoder(req.Body).Decode(&patientProfile); err != nil {
		h.writeResult(w, nil, fmt.Errorf("cannot decode body: %w", err))
		return
	}

	result, err := h.service.InsertPatient(req.Context(), patientProfile)
	if err != nil {
		h.writeResult(w, nil, fmt.Errorf("cannot insert patient: %w", err))
		return
	}

	h.writeResult(w, result, nil)
}

type httpResult struct {
	Result       interface{} `json:"result"`
	ErrorMessage string      `json:"errorMessage"`
}

func (h *handler) writeResult(w http.ResponseWriter, result interface{}, err error) {
	res := httpResult{
		Result: result,
	}
	if err != nil {
		res.ErrorMessage = err.Error()
	}

	w.Header().Add("Content-Type", "application/json")
	if err == nil {
		w.WriteHeader(200)
	} else {
		w.WriteHeader(400)
	}

	// TODO (jauhararifin): log the error here.
	json.NewEncoder(w).Encode(res)
}

func (h *handler) handleSearchPatient(w http.ResponseWriter, req *http.Request) {
	query := req.FormValue("q")
	cursor := req.FormValue("cursor")

	result, err := h.service.SearchPatient(req.Context(), query, patient.SearchOption{Cursor: cursor, BatchSize: 30})
	if err != nil {
		h.writeResult(w, nil, fmt.Errorf("cannot search patient: %w", err))
		return
	}

	h.writeResult(w, result, nil)
}

func (h *handler) handleGetPatient(w http.ResponseWriter, req *http.Request) {
	patientID := mux.Vars(req)["patientID"]

	patient, err := h.service.GetPatient(req.Context(), patientID)
	if err != nil {
		h.writeResult(w, nil, fmt.Errorf("cannot get patient: %w", err))
		return
	}

	h.writeResult(w, patient, nil)
}

func (h *handler) handleUpdatePatient(w http.ResponseWriter, req *http.Request) {
	patientID := mux.Vars(req)["patientID"]
	patientProfile := patient.Profile{}
	if err := json.NewDecoder(req.Body).Decode(&patientProfile); err != nil {
		h.writeResult(w, nil, fmt.Errorf("cannot decode body: %w", err))
		return
	}
	patientProfile.ID = patientID

	err := h.service.UpdatePatient(req.Context(), patientProfile)
	if err != nil {
		h.writeResult(w, nil, fmt.Errorf("cannot insert patient: %w", err))
		return
	}

	h.writeResult(w, nil, nil)
}

func NewHandler(service patient.Service, logger log.Log) *handler {
	return &handler{service: service, logger: logger}
}
