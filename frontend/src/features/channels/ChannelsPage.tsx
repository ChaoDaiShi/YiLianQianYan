import PageShell from "../../components/layout/PageShell";
import ChannelsPanel from "./ChannelsPanel";

export default function ChannelsPage() {
  return <PageShell variant="workspace" maxWidth="none" contentClassName="p-0"><ChannelsPanel/></PageShell>;
}
