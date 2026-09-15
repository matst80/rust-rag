"use client";

import { useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { useItemShare, useCreateShare, useRevokeShare } from "@/lib/api";
import { Share2, Copy, Check, ExternalLink, Trash2, Loader2, Globe } from "lucide-react";
import { toast } from "sonner";

interface ShareModalProps {
  itemId: string;
  itemTitle?: string;
  isOpen: boolean;
  onClose: () => void;
}

export function ShareModal({ itemId, itemTitle, isOpen, onClose }: ShareModalProps) {
  const { data: share, isLoading } = useItemShare(isOpen ? itemId : null);
  const { trigger: createShare, isMutating: isCreating } = useCreateShare(itemId);
  const { trigger: revokeShare, isMutating: isRevoking } = useRevokeShare(itemId);
  const [copied, setCopied] = useState(false);

  const getShareUrl = (token: string) => {
    if (typeof window !== "undefined") {
      return `${window.location.origin}/public/${token}`;
    }
    return `/public/${token}`;
  };

  const handleCopy = async (url: string) => {
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      toast.success("Public share link copied to clipboard");
      setTimeout(() => setCopied(false), 2000);
    } catch {
      toast.error("Failed to copy link");
    }
  };

  const handleCreate = async () => {
    try {
      const res = await createShare();
      toast.success("Public share link created");
      if (res?.token) {
        handleCopy(getShareUrl(res.token));
      }
    } catch (err) {
      toast.error("Failed to create share link");
    }
  };

  const handleRevoke = async () => {
    try {
      await revokeShare();
      toast.success("Share link revoked");
    } catch (err) {
      toast.error("Failed to revoke share link");
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-base font-semibold">
            <Globe className="size-4 text-primary" />
            Share Entry Publicly
          </DialogTitle>
          <DialogDescription className="text-xs text-muted-foreground">
            Anyone with this link will be able to read this document without logging in.
          </DialogDescription>
        </DialogHeader>

        <div className="py-2 space-y-4">
          {isLoading ? (
            <div className="flex items-center justify-center py-6">
              <Loader2 className="size-6 animate-spin text-muted-foreground" />
            </div>
          ) : share ? (
            <div className="space-y-3">
              <div className="flex items-center gap-2 p-2 rounded border border-border bg-muted/30">
                <input
                  type="text"
                  readOnly
                  value={getShareUrl(share.token)}
                  className="w-full bg-transparent font-mono text-xs text-foreground focus:outline-none select-all truncate"
                />
                <Button
                  size="icon"
                  variant="ghost"
                  className="size-8 shrink-0"
                  onClick={() => handleCopy(getShareUrl(share.token))}
                  title="Copy link"
                >
                  {copied ? (
                    <Check className="size-3.5 text-emerald-500" />
                  ) : (
                    <Copy className="size-3.5" />
                  )}
                </Button>
                <Button
                  size="icon"
                  variant="ghost"
                  className="size-8 shrink-0"
                  asChild
                  title="Open public link"
                >
                  <a href={`/public/${share.token}`} target="_blank" rel="noopener noreferrer">
                    <ExternalLink className="size-3.5" />
                  </a>
                </Button>
              </div>

              <div className="flex items-center justify-between pt-2">
                <span className="text-[11px] text-muted-foreground">
                  Active link (read-only)
                </span>
                <Button
                  variant="destructive"
                  size="sm"
                  className="h-7 text-[11px] font-mono"
                  onClick={handleRevoke}
                  disabled={isRevoking}
                >
                  {isRevoking ? (
                    <Loader2 className="size-3 mr-1 animate-spin" />
                  ) : (
                    <Trash2 className="size-3 mr-1" />
                  )}
                  Revoke Link
                </Button>
              </div>
            </div>
          ) : (
            <div className="py-4 text-center space-y-3">
              <div className="size-10 mx-auto rounded-full bg-muted flex items-center justify-center text-muted-foreground">
                <Share2 className="size-5" />
              </div>
              <p className="text-xs text-muted-foreground max-w-xs mx-auto">
                No active public share link for this entry. Creating a link allows anyone with the URL to view this note.
              </p>
              <Button
                onClick={handleCreate}
                disabled={isCreating}
                className="font-mono text-xs"
              >
                {isCreating ? (
                  <Loader2 className="size-3.5 mr-2 animate-spin" />
                ) : (
                  <Share2 className="size-3.5 mr-2" />
                )}
                Generate Share Link
              </Button>
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
