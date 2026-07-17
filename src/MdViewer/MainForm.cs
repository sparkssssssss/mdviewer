using System.Text.Json;
using Microsoft.Web.WebView2.Core;
using Microsoft.Web.WebView2.WinForms;

namespace MdViewer;

public sealed class MainForm : Form
{
    private readonly WebView2 _webView = new() { Dock = DockStyle.Fill };
    private readonly ToolStripStatusLabel _statusLabel = new() { Text = "Ready" };
    private string? _currentFile;
    private bool _viewerReady;

    public MainForm(string? initialFile)
    {
        Text = "MdViewer";
        Width = 1100;
        Height = 760;
        MinimumSize = new Size(720, 480);
        AllowDrop = true;
        _currentFile = NormalizeFilePath(initialFile);

        Controls.Add(_webView);
        Controls.Add(CreateStatusStrip());
        Controls.Add(CreateMenu());
        MainMenuStrip = Controls.OfType<MenuStrip>().FirstOrDefault();

        DragEnter += OnDragEnter;
        DragDrop += OnDragDrop;
        Shown += async (_, _) => await InitializeWebViewAsync();
    }

    private MenuStrip CreateMenu()
    {
        var menu = new MenuStrip();
        var file = new ToolStripMenuItem("File");
        file.DropDownItems.Add("Open...", null, async (_, _) => await OpenFileAsync());
        file.DropDownItems.Add("Reload", null, async (_, _) => await RenderCurrentFileAsync());
        file.DropDownItems.Add(new ToolStripSeparator());
        file.DropDownItems.Add("Exit", null, (_, _) => Close());

        var view = new ToolStripMenuItem("View");
        view.DropDownItems.Add("Toggle Theme", null, async (_, _) => await ExecuteScriptAsync("window.mdviewer?.toggleTheme?.()"));
        view.DropDownItems.Add("Zoom In", null, (_, _) => _webView.ZoomFactor += 0.1);
        view.DropDownItems.Add("Zoom Out", null, (_, _) => _webView.ZoomFactor = Math.Max(0.5, _webView.ZoomFactor - 0.1));
        view.DropDownItems.Add("Reset Zoom", null, (_, _) => _webView.ZoomFactor = 1.0);

        menu.Items.Add(file);
        menu.Items.Add(view);
        return menu;
    }

    private StatusStrip CreateStatusStrip()
    {
        var status = new StatusStrip();
        status.Items.Add(_statusLabel);
        return status;
    }

    private async Task InitializeWebViewAsync()
    {
        try
        {
            await _webView.EnsureCoreWebView2Async();
            var assetsDir = Path.Combine(AppContext.BaseDirectory, "assets");
            _webView.CoreWebView2.SetVirtualHostNameToFolderMapping(
                "mdviewer.local",
                assetsDir,
                CoreWebView2HostResourceAccessKind.Allow);

            _webView.CoreWebView2.WebMessageReceived += async (_, e) =>
            {
                var message = e.TryGetWebMessageAsString();
                if (message == "viewer-ready")
                {
                    _viewerReady = true;
                    await RenderCurrentFileAsync();
                }
                else if (message == "edit-source")
                {
                    OpenCurrentFileForEdit();
                }
            };

            _webView.CoreWebView2.NewWindowRequested += (_, e) =>
            {
                e.Handled = true;
                TryOpenExternal(e.Uri);
            };

            _webView.CoreWebView2.NavigationStarting += (_, e) =>
            {
                if (!e.Uri.StartsWith("https://mdviewer.local/", StringComparison.OrdinalIgnoreCase) &&
                    !e.Uri.StartsWith("https://mdviewer-files.local/", StringComparison.OrdinalIgnoreCase))
                {
                    e.Cancel = true;
                    TryOpenExternal(e.Uri);
                }
            };

            _webView.Source = new Uri("https://mdviewer.local/viewer.html");
        }
        catch (Exception ex)
        {
            MessageBox.Show(
                "Failed to initialize WebView2. Please install Microsoft Edge WebView2 Runtime.\n\n" + ex.Message,
                "MdViewer",
                MessageBoxButtons.OK,
                MessageBoxIcon.Error);
        }
    }

    private async Task OpenFileAsync()
    {
        using var dialog = new OpenFileDialog
        {
            Filter = "Markdown files (*.md;*.markdown;*.mdown)|*.md;*.markdown;*.mdown|Text files (*.txt)|*.txt|All files (*.*)|*.*",
            Title = "Open Markdown file"
        };

        if (dialog.ShowDialog(this) == DialogResult.OK)
        {
            _currentFile = dialog.FileName;
            await RenderCurrentFileAsync();
        }
    }

    private async Task RenderCurrentFileAsync()
    {
        if (!_viewerReady)
        {
            return;
        }

        if (string.IsNullOrWhiteSpace(_currentFile))
        {
            Text = "MdViewer";
            _statusLabel.Text = "Drop a Markdown file here or use File > Open";
            await PostJsonAsync(new ViewerPayload(null, "# MdViewer\n\nOpen or drag a Markdown file to preview it.\n\n```mermaid\ngraph TD\n  A[Markdown] --> B[Mermaid]\n  A --> C[KaTeX]\n```\n\nInline math: $E = mc^2$", null));
            return;
        }

        try
        {
            var text = await File.ReadAllTextAsync(_currentFile);
            var baseDir = Path.GetDirectoryName(_currentFile);
            if (!string.IsNullOrWhiteSpace(baseDir))
            {
                _webView.CoreWebView2.SetVirtualHostNameToFolderMapping(
                    "mdviewer-files.local",
                    baseDir,
                    CoreWebView2HostResourceAccessKind.Allow);
            }

            Text = Path.GetFileName(_currentFile) + " - MdViewer";
            _statusLabel.Text = _currentFile;
            await PostJsonAsync(new ViewerPayload(Path.GetFileName(_currentFile), text, baseDir));
        }
        catch (Exception ex)
        {
            _statusLabel.Text = ex.Message;
            await PostJsonAsync(new ViewerPayload("Error", "# Failed to open file\n\n```text\n" + ex.Message + "\n```", null));
        }
    }

    private async Task PostJsonAsync(ViewerPayload payload)
    {
        var json = JsonSerializer.Serialize(payload);
        await ExecuteScriptAsync($"window.mdviewer?.render({json})");
    }

    private async Task ExecuteScriptAsync(string script)
    {
        if (_webView.CoreWebView2 is not null)
        {
            await _webView.ExecuteScriptAsync(script);
        }
    }

    private static string? NormalizeFilePath(string? path)
    {
        if (string.IsNullOrWhiteSpace(path))
        {
            return null;
        }

        try
        {
            return Path.GetFullPath(path.Trim('"'));
        }
        catch
        {
            return path;
        }
    }

    private void OpenCurrentFileForEdit()
    {
        if (string.IsNullOrWhiteSpace(_currentFile) || !File.Exists(_currentFile))
        {
            _statusLabel.Text = "No source file to edit";
            return;
        }

        if (TryStartProcess("code.cmd", "--goto " + QuoteArgument(_currentFile + ":1:1")) ||
            TryStartProcess("code", "--goto " + QuoteArgument(_currentFile + ":1:1")) ||
            TryStartProcess("notepad.exe", QuoteArgument(_currentFile)))
        {
            _statusLabel.Text = "Editing " + _currentFile;
            return;
        }

        TryOpenExternal(_currentFile);
    }

    private static bool TryStartProcess(string fileName, string arguments)
    {
        try
        {
            System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo(fileName, arguments)
            {
                UseShellExecute = false
            });
            return true;
        }
        catch
        {
            return false;
        }
    }

    private static string QuoteArgument(string value)
    {
        return "\"" + value.Replace("\"", "\\\"") + "\"";
    }

    private static void TryOpenExternal(string uri)
    {
        try
        {
            System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo(uri) { UseShellExecute = true });
        }
        catch
        {
            // Ignore external launch failures.
        }
    }

    private async void OnDragDrop(object? sender, DragEventArgs e)
    {
        if (e.Data?.GetData(DataFormats.FileDrop) is string[] files && files.Length > 0)
        {
            _currentFile = files[0];
            await RenderCurrentFileAsync();
        }
    }

    private static void OnDragEnter(object? sender, DragEventArgs e)
    {
        e.Effect = e.Data?.GetDataPresent(DataFormats.FileDrop) == true ? DragDropEffects.Copy : DragDropEffects.None;
    }

    private sealed record ViewerPayload(string? Title, string Markdown, string? BasePath);
}
