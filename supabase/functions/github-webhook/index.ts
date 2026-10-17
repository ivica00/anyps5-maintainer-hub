import { serve } from "https://deno.land/std@0.177.0/http/server.ts";
import { createClient } from "https://esm.sh/@supabase/supabase-js@2.39.7";

function isBoilerplateFile(path: string): boolean {
  const p = path.toLowerCase().replace(/\\/g, "/");
  return p.endsWith("cmakelists.txt") || p.includes("docs/dev/debt/");
}

function classifyPR(paths: string[], title: string): string {
  const pLower = paths.map((p) => p.toLowerCase());
  const tLower = title.toLowerCase();

  if (pLower.some((p) => p.includes("agc") || p.includes("vulkandevice")) || tLower.includes("agc")) {
    return "AGC & Graphics";
  }
  if (
    pLower.some((p) => p.includes("shader") || p.includes("relinker")) ||
    tLower.includes("shader") ||
    tLower.includes("relinker")
  ) {
    return "Shader & Relinker";
  }
  if (
    pLower.some(
      (p) =>
        p.includes("libkernel") ||
        p.includes("kqueue") ||
        p.includes("directmemory") ||
        p.includes("virtualmemory") ||
        p.includes("vm.cpp"),
    ) ||
    tLower.includes("kernel")
  ) {
    return "Kernel & Syscalls";
  }
  if (
    pLower.some(
      (p) =>
        p.includes("/libc/") ||
        p.includes("/libc.") ||
        p.includes("libscelibcinternal") ||
        p.includes("/libm/") ||
        p.includes("libunwind"),
    ) ||
    tLower.includes("libc")
  ) {
    return "LibC & Base Runtime";
  }
  if (
    pLower.some((p) => p.includes("core/libs/prx/")) ||
    tLower.startsWith("feat(libsce") ||
    tLower.startsWith("fix(libsce")
  ) {
    return "PRX High-Level Modules";
  }
  if (pLower.some((p) => p.includes("tools/") || p.includes(".github/") || p.endsWith("cmakelists.txt"))) {
    return "Build & Tooling";
  }
  if (pLower.every((p) => p.includes("docs/")) || tLower.startsWith("docs")) {
    return "Documentation & Debt";
  }
  return "Other Core Subsystems";
}

serve(async (req: Request) => {
  if (req.method !== "POST") {
    return new Response(JSON.stringify({ error: "Method not allowed" }), {
      status: 405,
      headers: { "Content-Type": "application/json" },
    });
  }

  const supabaseUrl = Deno.env.get("SUPABASE_URL") || "";
  const supabaseServiceKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY") || "";

  if (!supabaseUrl || !supabaseServiceKey) {
    return new Response(JSON.stringify({ error: "Missing Supabase credentials in Edge Function environment" }), {
      status: 500,
      headers: { "Content-Type": "application/json" },
    });
  }

  const supabase = createClient(supabaseUrl, supabaseServiceKey);

  try {
    const payload = await req.json();
    const action = payload.action;
    const pr = payload.pull_request;

    // We only process pull_request events
    if (!pr || !pr.number) {
      return new Response(JSON.stringify({ message: "Ignored non-PR event" }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    }

    const prNumber = pr.number;
    console.log(`Received GitHub webhook for PR #${prNumber}, action: ${action}`);

    // If PR was closed or merged, delete from open prs table (preserving claimed batch safety)
    if (action === "closed") {
      const isMerged = Boolean(pr.merged_at || pr.merged);
      const newState = isMerged ? "merged" : "closed";

      // Check if PR is in any batch
      const { data: batches } = await supabase
        .from("batches")
        .select("id, pr_numbers")
        .contains("pr_numbers", [prNumber]);

      if (batches && batches.length > 0) {
        // If part of a batch, keep row with updated github_state for lock & progress tracking
        await supabase
          .from("prs")
          .update({
            github_state: newState,
            updated_at: new Date().toISOString(),
          })
          .eq("number", prNumber);
        console.log(`PR #${prNumber} marked as ${newState} (preserved in batch)`);
      } else {
        // If not in a batch, remove row from database
        await supabase.from("prs").delete().eq("number", prNumber);
        console.log(`PR #${prNumber} removed from database (${newState})`);
      }

      return new Response(JSON.stringify({ success: true, action: "closed/merged", number: prNumber }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    }

    // For opened, synchronize, reopened, edited: fetch files if not provided in payload
    let paths: string[] = [];
    if (Array.isArray(payload.files)) {
      paths = payload.files.map((f: any) => (typeof f === "string" ? f : f.filename || f.path));
    } else {
      // Fetch files from GitHub API
      try {
        const ghToken = Deno.env.get("GITHUB_TOKEN");
        const headers: Record<string, string> = {
          "Accept": "application/vnd.github.v3+json",
          "User-Agent": "AnyPS5-Supabase-Webhook",
        };
        if (ghToken) {
          headers["Authorization"] = `Bearer ${ghToken}`;
        }
        const ghRes = await fetch(
          `https://api.github.com/repos/boykopovar/AnyPS5/pulls/${prNumber}/files?per_page=100`,
          { headers },
        );
        if (ghRes.ok) {
          const filesData = await ghRes.json();
          if (Array.isArray(filesData)) {
            paths = filesData.map((f: any) => f.filename);
          }
        }
      } catch (e) {
        console.warn(`Failed to fetch files from GitHub API for PR #${prNumber}:`, e);
      }
    }

    const title = pr.title || "";
    const adds = pr.additions || 0;
    const dels = pr.deletions || 0;
    const tot = adds + dels;
    const codePaths = paths.filter((p) => !isBoilerplateFile(p));
    const effFileCount = codePaths.length > 0 ? codePaths.length : paths.length;

    let risk = "low";
    if (effFileCount > 10 || tot > 1500) {
      risk = "high";
    } else if (effFileCount >= 4 || tot > 300) {
      risk = "medium";
    }

    const size = tot < 25 ? "XS" : tot <= 100 ? "S" : tot <= 400 ? "M" : tot <= 1200 ? "L" : "XL";
    const authorLogin = pr.user?.login || "ghost";
    const category = classifyPR(paths, title);

    const record = {
      number: prNumber,
      title: title,
      author: authorLogin,
      author_avatar: `https://github.com/${authorLogin}.png?size=64`,
      created_at: pr.created_at || new Date().toISOString(),
      url: pr.html_url || `https://github.com/boykopovar/AnyPS5/pull/${prNumber}`,
      additions: adds,
      deletions: dels,
      total_lines: tot,
      file_count: pr.changed_files || paths.length,
      files: paths,
      category: category,
      hotspots: [],
      risk_level: risk,
      size: size,
      is_draft: Boolean(pr.draft),
      head_sha: pr.head?.sha || "",
      github_state: "open",
      mergeable_state: pr.mergeable_state || "unknown",
      updated_at: new Date().toISOString(),
    };

    const { error: upsertErr } = await supabase.from("prs").upsert(record, { onConflict: "number" });
    if (upsertErr) {
      console.error(`Failed to upsert PR #${prNumber}:`, upsertErr);
      return new Response(JSON.stringify({ error: upsertErr.message }), {
        status: 500,
        headers: { "Content-Type": "application/json" },
      });
    }

    console.log(`PR #${prNumber} successfully updated/upserted in real time.`);
    return new Response(JSON.stringify({ success: true, number: prNumber, category }), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    });
  } catch (err: any) {
    console.error("Webhook processing error:", err);
    return new Response(JSON.stringify({ error: err.message }), {
      status: 400,
      headers: { "Content-Type": "application/json" },
    });
  }
});
