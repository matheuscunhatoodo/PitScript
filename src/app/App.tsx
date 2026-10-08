import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AppShell, type Page } from "../components/AppShell";
import { HomePage } from "../pages/HomePage";
import { MeetingPage } from "../pages/MeetingPage";
import { NewRecordingPage } from "../pages/NewRecordingPage";
import { SettingsPage } from "../pages/SettingsPage";
import "../styles/app.css";
import { useRecording } from "../hooks/useRecording";
import { RecordingBar } from "../components/RecordingBar";

export default function App() {
  const recording = useRecording();
  const [page, setPage] = useState<Page>("home");
  const [selectedMeetingId, setSelectedMeetingId] = useState<string | null>(
    null,
  );

  useEffect(() => {
    window.scrollTo(0, 0);
  }, [page, selectedMeetingId]);

  useEffect(() => {
    if (!isTauri()) return;
    let mounted = true;
    const subscription = listen("tray-open-state", () => {
      if (mounted) setPage("new");
    }).catch(() => () => {});
    return () => {
      mounted = false;
      void subscription.then((unlisten) => unlisten());
    };
  }, []);

  function openMeeting(id: string) {
    setSelectedMeetingId(id);
    setPage("meeting");
  }

  return (
    <AppShell
      page={page}
      onNavigate={setPage}
      recording={!!recording.capture?.active}
    >
      {recording.error && page !== "new" && (
        <div className="inline-notice" role="alert">
          {recording.error}
          <button
            className="text-button"
            onClick={() => void recording.refresh()}
          >
            Atualizar estado
          </button>
        </div>
      )}
      {page === "home" && (
        <HomePage
          onNewRecording={() => setPage("new")}
          onOpenMeeting={openMeeting}
        />
      )}
      {page === "new" && (
        <NewRecordingPage onOpenMeeting={openMeeting} recording={recording} />
      )}
      {page === "meeting" && selectedMeetingId && (
        <MeetingPage
          key={selectedMeetingId}
          id={selectedMeetingId}
          onBack={() => setPage("home")}
        />
      )}
      {page === "settings" && <SettingsPage />}
      <RecordingBar
        recording={recording}
        onShow={() => setPage("new")}
        onOpenMeeting={openMeeting}
        showStop={page !== "new"}
      />
    </AppShell>
  );
}
