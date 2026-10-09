using ClaudeCodeUI;
using ClaudeCodeUI.Components;

if (args is ["--self-check"]) { Environment.ExitCode = SelfCheck.Run(); return; }

var builder = WebApplication.CreateBuilder(args);

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
    app.UseHsts();
}
app.UseStatusCodePagesWithReExecute("/not-found", createScopeForStatusCodePages: true);
app.UseHttpsRedirection();

app.UseAntiforgery();

app.MapStaticAssets();
app.MapRazorComponents<App>()
    .AddInteractiveServerRenderMode();

app.Run();
