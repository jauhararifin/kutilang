package log

import (
	"context"
	"net/http"
	"strings"
	"time"

	"github.com/google/uuid"
)

func NewMiddleware(l Log) func (http.Handler) http.Handler {
	return func(h http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			reqID := strings.Replace(uuid.New().String(), "-", "", -1)
			r = r.WithContext(context.WithValue(r.Context(), "requestID", reqID))

			defer func(t time.Time) {
				l.Log(r.Context(), "access", "latency", time.Since(t), "path", r.URL.String(), "method", r.Method)
			}(time.Now())

			h.ServeHTTP(w, r)
		})
	}
}

