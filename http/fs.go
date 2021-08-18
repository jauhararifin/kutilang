package http

import (
	"embed"
	"io/fs"
	"net/http"
	"os"
	"path"
)

//go:embed web
var staticFiles embed.FS

type fsFunc func(name string) (fs.File, error)

func (f fsFunc) Open(name string) (fs.File, error) {
	return f(name)
}

func staticFileHandler() http.Handler {
	handler := fsFunc(func(name string) (fs.File, error) {
		f, err := staticFiles.Open(path.Join("web", name))
		if os.IsNotExist(err) {
			return staticFiles.Open("web/index.html")
		}
		return f, err
	})
	return http.StripPrefix("/", http.FileServer(http.FS(handler)))
}
