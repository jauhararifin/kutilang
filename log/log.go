package log

import (
	"context"

	"github.com/sirupsen/logrus"
)

type Log interface {
	Log(ctx context.Context, msg string, args ...interface{})
}

type log struct {
	logger *logrus.Logger
}

func (l *log) Log(ctx context.Context, msg string, args ...interface{}) {
	entry := logrus.NewEntry(l.logger)
	reqID := ctx.Value("requestID")
	if reqID != nil {
		entry = entry.WithField("requestID", reqID)
	}
	for i := 0; i < len(args); i += 2 {
		entry = entry.WithField(args[i].(string), args[i+1])
	}
	entry.Info(msg)
}

func New(logger *logrus.Logger) *log {
	return &log{logger: logger}
}
