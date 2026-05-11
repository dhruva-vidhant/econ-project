import { Route, Routes } from "react-router-dom";

import HomePage from "@/features/home/HomePage";
import CompanyDashboardPage from "@/features/dashboard/CompanyDashboardPage";
import StatementPage from "@/features/dashboard/StatementPage";
import MetricDrillPage from "@/features/dashboard/MetricDrillPage";
import DiagnosticsPage from "@/features/diagnostics/DiagnosticsPage";
import AppShell from "@/components/AppShell";

export default function App() {
  return (
    <AppShell>
      <Routes>
        <Route path="/" element={<HomePage />} />
        <Route path="/c/:ticker" element={<CompanyDashboardPage />} />
        <Route path="/c/:ticker/statement/:kind" element={<StatementPage />} />
        <Route path="/c/:ticker/metric/:metric" element={<MetricDrillPage />} />
        <Route path="/c/:ticker/diagnostics" element={<DiagnosticsPage />} />
      </Routes>
    </AppShell>
  );
}
