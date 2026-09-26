import { Input, Textarea } from "../../../components/ui";

interface BasicSectionProps {
  title: string;
  onTitleChange: (value: string) => void;
  instruction: string;
  onInstructionChange: (value: string) => void;
}

/** What the node is: its title and the instruction it must carry out. */
export default function BasicSection({
  title,
  onTitleChange,
  instruction,
  onInstructionChange,
}: BasicSectionProps) {
  return (
    <>
      <Input
        label="标题"
        value={title}
        onChange={(event) => onTitleChange(event.target.value)}
        required
        maxLength={200}
      />
      <Textarea
        label="任务说明"
        value={instruction}
        onChange={(event) => onInstructionChange(event.target.value)}
        rows={4}
      />
    </>
  );
}
