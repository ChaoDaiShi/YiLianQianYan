import { Component, type ReactNode } from "react";
import { Button, ErrorState } from "../../components/ui";

export default class MemorySpaceBoundary extends Component<{children: ReactNode; onReturn: () => void}, {failed: boolean}> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  render() {
    if (!this.state.failed) return this.props.children;
    return <div className="memory-space-failure">
      <ErrorState title="立体图暂时无法显示" description="可以通过二维图或列表查看这批记忆。" action={
        <Button size="sm" onClick={this.props.onReturn}>回到二维图</Button>} />
    </div>;
  }
}
