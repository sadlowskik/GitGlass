import { useEffect } from "react";
import { TitleBar } from "./components/TitleBar";
import { Sidebar } from "./components/Sidebar";
import { Breadcrumbs } from "./components/Breadcrumbs";
import { FileList } from "./components/FileList";
import { CommitPanel } from "./components/CommitPanel";
import { SecretGuardDialog } from "./components/SecretGuardDialog";
import { DiffViewer } from "./components/DiffViewer";
import { AutomationsDialog } from "./components/AutomationsDialog";
import { InitRepoDialog } from "./components/InitRepoDialog";
import { IdentityDialog } from "./components/IdentityDialog";
import { DiscardDialog } from "./components/DiscardDialog";
import { GithubDialogs } from "./components/GithubDialogs";
import { StatusLegend } from "./components/StatusLegend";
import { Toaster } from "./components/Toaster";
import { useAppStore } from "./store/useAppStore";
import { useGithubStore } from "./store/useGithubStore";

export default function App() {
  const bootstrap = useAppStore((s) => s.bootstrap);
  const loadGithubStatus = useGithubStore((s) => s.loadStatus);
  const refreshGithubLists = useGithubStore((s) => s.refreshLists);
  // Re-fetch PRs/issues whenever we land in a different repo.
  const repoRoot = useAppStore((s) => s.listing?.repo?.root ?? null);

  useEffect(() => {
    void bootstrap();
    void loadGithubStatus();
  }, [bootstrap, loadGithubStatus]);

  useEffect(() => {
    void refreshGithubLists();
  }, [repoRoot, refreshGithubLists]);

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-surface-0 text-content">
      <TitleBar />
      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <main className="flex min-w-0 flex-1 flex-col">
          <Breadcrumbs />
          <FileList />
          <CommitPanel />
          <StatusLegend />
        </main>
      </div>
      <DiffViewer />
      <AutomationsDialog />
      <InitRepoDialog />
      <SecretGuardDialog />
      <IdentityDialog />
      <DiscardDialog />
      <GithubDialogs />
      <Toaster />
    </div>
  );
}
