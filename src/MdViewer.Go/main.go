package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"time"

	webview "github.com/webview/webview_go"
)

type viewerPayload struct {
	Title       string `json:"Title"`
	Markdown    string `json:"Markdown"`
	BasePath    string `json:"BasePath"`
	FileBaseUrl string `json:"FileBaseUrl"`
}

type appState struct {
	currentFile string
	assetsDir   string
	server      *http.Server
	baseURL     string
	webview     webview.WebView
}

func main() {
	state, err := newAppState()
	if err != nil {
		log.Fatal(err)
	}
	defer state.shutdown()

	debug := false
	w := webview.New(debug)
	defer w.Destroy()
	state.webview = w

	w.SetTitle("MdViewer Go")
	w.SetSize(1100, 760, webview.HintNone)
	bind(w, "viewerReady", func() {
		state.renderCurrentFile()
	})
	bind(w, "editSource", func() {
		state.openCurrentFileForEdit()
	})
	bind(w, "openFile", func(path string) {
		state.currentFile = normalizePath(path)
		state.renderCurrentFile()
	})

	w.Navigate(state.baseURL + "/assets/viewer.html")
	w.Run()
}

func newAppState() (*appState, error) {
	exe, err := os.Executable()
	if err != nil {
		return nil, err
	}
	exeDir := filepath.Dir(exe)
	assetsDir := filepath.Join(exeDir, "assets")
	if _, err := os.Stat(filepath.Join(assetsDir, "viewer.html")); err != nil {
		// Development fallback: go run from repo root or src/MdViewer.Go.
		candidates := []string{
			filepath.Join("..", "MdViewer", "assets"),
			filepath.Join("src", "MdViewer", "assets"),
		}
		for _, candidate := range candidates {
			abs, _ := filepath.Abs(candidate)
			if _, statErr := os.Stat(filepath.Join(abs, "viewer.html")); statErr == nil {
				assetsDir = abs
				break
			}
		}
	}

	state := &appState{assetsDir: assetsDir}
	if len(os.Args) > 1 && !strings.HasPrefix(os.Args[1], "-") {
		state.currentFile = normalizePath(os.Args[1])
	}
	if err := state.startServer(); err != nil {
		return nil, err
	}
	return state, nil
}

func (a *appState) startServer() error {
	mux := http.NewServeMux()
	mux.Handle("/assets/", http.StripPrefix("/assets/", http.FileServer(http.Dir(a.assetsDir))))
	mux.HandleFunc("/file/", a.handleLocalFile)

	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return err
	}
	a.baseURL = "http://" + listener.Addr().String()
	a.server = &http.Server{Handler: mux}
	go func() {
		if err := a.server.Serve(listener); err != nil && !errors.Is(err, http.ErrServerClosed) {
			log.Printf("server error: %v", err)
		}
	}()
	return nil
}

func (a *appState) handleLocalFile(w http.ResponseWriter, r *http.Request) {
	if a.currentFile == "" {
		http.NotFound(w, r)
		return
	}
	baseDir := filepath.Dir(a.currentFile)
	rel := strings.TrimPrefix(r.URL.Path, "/file/")
	rel = strings.ReplaceAll(rel, "/", string(filepath.Separator))
	requested := filepath.Clean(filepath.Join(baseDir, rel))

	baseClean := filepath.Clean(baseDir)
	if requested != baseClean && !strings.HasPrefix(requested, baseClean+string(filepath.Separator)) {
		http.Error(w, "forbidden", http.StatusForbidden)
		return
	}
	http.ServeFile(w, r, requested)
}

func (a *appState) renderCurrentFile() {
	if a.webview == nil {
		return
	}

	payload := viewerPayload{
		Title:       "MdViewer",
		Markdown:    "# MdViewer Go\n\nOpen a Markdown file from command line to preview it.",
		FileBaseUrl: a.baseURL + "/file/",
	}

	if a.currentFile != "" {
		data, err := os.ReadFile(a.currentFile)
		if err != nil {
			payload.Title = "Error"
			payload.Markdown = "# Failed to open file\n\n```text\n" + err.Error() + "\n```"
		} else {
			payload.Title = filepath.Base(a.currentFile)
			payload.Markdown = string(data)
			payload.BasePath = filepath.Dir(a.currentFile)
		}
	}

	encoded, err := json.Marshal(payload)
	if err != nil {
		return
	}
	a.webview.Dispatch(func() {
		a.webview.SetTitle(titleFor(payload.Title))
		a.webview.Eval("window.mdviewer && window.mdviewer.render(" + string(encoded) + ")")
	})
}

func titleFor(title string) string {
	if title == "" || title == "MdViewer" {
		return "MdViewer Go"
	}
	return title + " - MdViewer Go"
}

func (a *appState) openCurrentFileForEdit() {
	if a.currentFile == "" {
		return
	}
	openWithEditor(a.currentFile)
}

func openWithEditor(path string) {
	if runtime.GOOS == "windows" {
		if tryStart("code.cmd", "--goto", path+":1:1") || tryStart("code", "--goto", path+":1:1") || tryStart("notepad.exe", path) {
			return
		}
		_ = exec.Command("rundll32", "url.dll,FileProtocolHandler", path).Start()
		return
	}
	if runtime.GOOS == "darwin" {
		_ = exec.Command("open", path).Start()
		return
	}
	_ = exec.Command("xdg-open", path).Start()
}

func tryStart(name string, args ...string) bool {
	cmd := exec.Command(name, args...)
	if err := cmd.Start(); err != nil {
		return false
	}
	return true
}

func normalizePath(path string) string {
	path = strings.Trim(path, "\"")
	abs, err := filepath.Abs(path)
	if err == nil {
		return abs
	}
	return path
}

func (a *appState) shutdown() {
	if a.server == nil {
		return
	}
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	_ = a.server.Shutdown(ctx)
}

func bind[T any](w webview.WebView, name string, fn T) {
	if err := w.Bind(name, fn); err != nil {
		_, _ = fmt.Fprintln(io.Discard, err)
	}
}
