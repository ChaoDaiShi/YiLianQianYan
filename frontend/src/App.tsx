import { BrowserRouter } from "react-router-dom";
import { ThemeProvider } from "./theme";
import AppRoot from "./surfaces/AppRoot";
import RouteErrorBoundary from "./components/layout/RouteErrorBoundary";

export default function App() {
  return (
    <ThemeProvider>
      <BrowserRouter>
        <RouteErrorBoundary>
          <AppRoot />
        </RouteErrorBoundary>
      </BrowserRouter>
    </ThemeProvider>
  );
}
