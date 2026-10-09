using ClaudeCodeUI;
using ClaudeCodeUI.Components;

if (args is ["--self-check"]) { Environment.ExitCode = SelfCheck.Run(); return; }

// Desktop shell (desktop/): `--desktop-port <port> --parent-pid <pid>`. Loopback HTTP only, Production,
// content root next to the executable, and stop (disposing SessionManager, so claude children die) when the shell exits.
var port = ArgValue("--desktop-port");
var desktop = port is not null;

var builder = WebApplication.CreateBuilder(new WebApplicationOptions
{
    Args = args,
    ContentRootPath = desktop ? AppContext.BaseDirectory : null,
    EnvironmentName = desktop ? Environments.Production : null,
});
if (desktop) builder.WebHost.UseUrls($"http://127.0.0.1:{int.Parse(port!)}");

builder.Services.AddRazorComponents()
    .AddInteractiveServerComponents();
builder.Services.AddSingleton<SessionManager>();   // IAsyncDisposable: kills the claude processes on shutdown
builder.Services.AddSingleton<WorktreeService>();
builder.Services.AddScoped<UiState>();

var app = builder.Build();

if (!app.Environment.IsDevelopment())
{
    app.UseExceptionHandler("/Error", createScopeForErrors: true);
    // The default HSTS value is 30 days. You may want to change this for production scenarios, see https://aka.ms/aspnetcore-hsts.
    if (!desktop) app.UseHsts();
}
app.UseStatusCodePagesWithReExecute("/not-found", createScopeForStatusCodePages: true);
if (!desktop) app.UseHttpsRedirection();

app.UseAntiforgery();

app.MapStaticAssets();
app.MapRazorComponents<App>()
    .AddInteractiveServerRenderMode();

// No stdin watcher: a pending synchronous read on an inherited stdin pipe blocks CreateProcess (git, claude) on Windows.
if (ArgValue("--parent-pid") is { } pid)
    _ = System.Diagnostics.Process.GetProcessById(int.Parse(pid)).WaitForExitAsync().ContinueWith(_ => app.Lifetime.StopApplication());

app.Run();

string? ArgValue(string name) => Array.IndexOf(args, name) is var i and >= 0 && i + 1 < args.Length ? args[i + 1] : null;
