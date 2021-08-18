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

func staticFileHandler(prefix, root string) http.Handler {
	handler := fsFunc(func(name string) (fs.File, error) {
		f, err := staticFiles.Open(path.Join(root, name))
		if os.IsNotExist(err) {
			return staticFiles.Open(root + "/index.html")
		}
		return f, err
	})
	return http.StripPrefix(prefix, http.FileServer(http.FS(handler)))
}
