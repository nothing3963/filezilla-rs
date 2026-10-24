import {
  Archive,
  Code2,
  File as FileBase,
  FileText,
  Film,
  Folder,
  Image as ImageIcon,
  Link2,
  Music,
} from "lucide-react";
import { fileCategory } from "../lib/format";

export function FileIcon({
  name,
  isDir,
  isSymlink,
  size = 16,
}: {
  name: string;
  isDir: boolean;
  isSymlink?: boolean;
  size?: number;
}) {
  const category = fileCategory(name, isDir);
  const Icon =
    category === "folder"
      ? Folder
      : category === "image"
        ? ImageIcon
        : category === "video"
          ? Film
          : category === "audio"
            ? Music
            : category === "archive"
              ? Archive
              : category === "code"
                ? Code2
                : category === "document"
                  ? FileText
                  : FileBase;

  return (
    <span className="relative inline-flex flex-none items-center justify-center">
      <Icon className={`file-icon ${category}`} size={size} strokeWidth={1.7} />
      {isSymlink && (
        <Link2
          size={8}
          strokeWidth={2.4}
          className="absolute -bottom-1 -right-1 rounded-sm bg-[var(--pane-bg)] text-[var(--accent)]"
        />
      )}
    </span>
  );
}
