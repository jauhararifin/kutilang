package log

import (
	"bytes"
	"context"
	"io"
	"net/http"
	"strings"
	"sync/atomic"
	"time"

	"github.com/google/uuid"
)

func NewMiddleware(l Log) func(http.Handler) http.Handler {
	return func(h http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			reqID := strings.Replace(uuid.New().String(), "-", "", -1)
			r = r.WithContext(context.WithValue(r.Context(), "requestID", reqID))

			bodyWrapper := &requestBodyWrapper{
				body: r.Body,
				buff: &bytes.Buffer{},
			}
			r.Body = bodyWrapper

			writer := &responseWriterWrapper{
				w: w,
				buff: &bytes.Buffer{},
			}

			defer func(t time.Time) {
				l.Log(
					r.Context(),
					"access",
					"latency", time.Since(t),
					"path", r.URL.String(),
					"method", r.Method,
					"requestHeaders", r.Header,
					"responseHeaders", w.Header(),
					"status", atomic.LoadInt64(&writer.statusCode),
					"requestBody", bodyWrapper.buff.String(),
					"responseBody", writer.buff.String(),
				)
			}(time.Now())

			h.ServeHTTP(writer, r)
		})
	}
}

type responseWriterWrapper struct {
	w http.ResponseWriter
	buff *bytes.Buffer
	statusCode int64
}

func (r *responseWriterWrapper) Header() http.Header {
	return r.w.Header()
}

func (r *responseWriterWrapper) Write(buff []byte) (int, error) {
	if r.buff.Len() < 1024 {
		reading := 1024 - r.buff.Len()
		if len(buff) < reading {
			reading = len(buff)
		}
		r.buff.Write(buff[:reading])
	}
	return r.w.Write(buff)
}

func (r *responseWriterWrapper) WriteHeader(statusCode int) {
	r.w.WriteHeader(statusCode)
	atomic.StoreInt64(&r.statusCode, int64(statusCode))
}

type requestBodyWrapper struct {
	body io.ReadCloser
	buff *bytes.Buffer
}

func (r *requestBodyWrapper) Read(p []byte) (n int, err error) {
	n, err = r.body.Read(p)
	if err == nil && r.buff.Len() < 1024 {
		reading := 1024 - r.buff.Len()
		if n < reading {
			reading = n
		}
		r.buff.Write(p[:reading])
	}
	return n, err
}

func (r *requestBodyWrapper) Close() error {
	return r.body.Close()
}
