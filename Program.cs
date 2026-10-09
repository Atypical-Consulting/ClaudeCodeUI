using System.Security.Cryptography;
using System.Text;
using ClaudeCodeUI;
using ClaudeCodeUI.Components;

if (args is ["--self-check"]) { Environment.ExitCode = SelfCheck.Run(); return; }
if (args is ["--boot-probe", var probeDir, ..])
{
    Environment.ExitCode = await BootProbe.Run(Path.GetFullPath(probeDir), args is [_, _, var sec] && int.TryParse(sec, out var n) ? n : 90);
    return;
}

// Desktop shell (desktop/): `--desktop-port 0 --parent-pid <pid>`. Loopback HTTP only, Production,
// content root next to the executable, and stop (disposing SessionManager, so claude children die) when the shell exits.
// Once listening it prints `http://127.0.0.1:<port>/?token=<secret>` on stdout (a pipe only the shell reads): the token
// buys an HttpOnly cookie that every request (pages, assets, /_blazor) must carry, so other local users or a
// DNS-rebinding page can't drive claude.
var port = ArgValue("--desktop-port");
var desktop = port is not null;

var builder = WebApplication.CreateBuilder(new WebApplicationOptions
{
    Args = args,
    ContentRootPath = desktop ? AppContext.BaseDirectory : null,
    EnvironmentName = desktop ? Environments.Production : null,
});
var token = desktop ? RandomNumberGenerator.GetHexString(64, lowercase: true) : null;
if (desktop)
{
    builder.WebHost.UseUrls($"http://127.0.0.1:{int.Parse(port!)}");
    builder.Configuration["AllowedHosts"] = "127.0.0.1"; // HostFiltering answers 400 to rebound Host headers
}

builder.Services.AddRazorComponents()
    .AddInteractiveServerComponents();
builder.Services.AddSingleton<SessionManager>();   // IAsyncDisposable: kills the claude processes on shutdown
builder.Services.AddSingleton<WorktreeService>();
builder.Services.AddScoped<UiState>();

var app = builder.Build();

if (desktop)
    app.Use(async (ctx, next) =>
    {
        if (IsToken(ctx.Request.Query["token"]))
            ctx.Response.Cookies.Append("ccui", token!, new CookieOptions { HttpOnly = true, SameSite = SameSiteMode.Strict });
        else if (!IsToken(ctx.Request.Cookies["ccui"]))
        {
            ctx.Response.StatusCode = StatusCodes.Status403Forbidden;
            return;
        }
        await next();
    });

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

if (!desktop) { app.Run(); return; }
app.Start();
Console.WriteLine($"{app.Urls.First()}/?token={token}"); // Kestrel reports the port it actually bound
app.WaitForShutdown();

bool IsToken(string? value) =>
    value is not null && CryptographicOperations.FixedTimeEquals(Encoding.UTF8.GetBytes(value), Encoding.UTF8.GetBytes(token!));
string? ArgValue(string name) => Array.IndexOf(args, name) is var i and >= 0 && i + 1 < args.Length ? args[i + 1] : null;
