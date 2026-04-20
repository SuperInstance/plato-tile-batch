"""Bulk tile processing."""

from dataclasses import dataclass

@dataclass
class BatchSummary:
    total: int
    added: int
    skipped: int
    errors: int
    by_domain: dict

class TileBatchProcessor:
    def __init__(self, dedup_threshold: float = 0.9):
        self.dedup_threshold = dedup_threshold

    def process(self, tiles: list[dict], min_confidence: float = 0.0,
                max_content_length: int = 100000, domains: list[str] = None) -> tuple[list[dict], BatchSummary]:
        added, skipped, errors, seen = [], 0, 0, []
        by_domain = {}
        for t in tiles:
            try:
                if t.get("confidence", 0) < min_confidence:
                    skipped += 1; continue
                content = t.get("content", "")
                if len(content) < 10 or len(content) > max_content_length:
                    skipped += 1; continue
                if domains and t.get("domain", "") not in domains:
                    skipped += 1; continue
                if self._is_dup(content, seen):
                    skipped += 1; continue
                seen.append(content)
                added.append(t)
                d = t.get("domain", "unknown")
                by_domain[d] = by_domain.get(d, 0) + 1
            except Exception:
                errors += 1
        summary = BatchSummary(total=len(tiles), added=len(added),
                               skipped=skipped, errors=errors, by_domain=by_domain)
        return added, summary

    def partition(self, tiles: list[dict], key: str = "domain") -> dict[str, list[dict]]:
        parts = {}
        for t in tiles:
            k = t.get(key, "unknown")
            if k not in parts: parts[k] = []
            parts[k].append(t)
        return parts

    def filter_by_priority(self, tiles: list[dict], priority: str) -> list[dict]:
        return [t for t in tiles if t.get("priority") == priority]

    def _is_dup(self, content: str, seen: list[str]) -> bool:
        words = set(content.lower().split())
        for s in seen[-200:]:
            s_words = set(s.lower().split())
            if words and s_words:
                jaccard = len(words & s_words) / len(words | s_words)
                if jaccard >= self.dedup_threshold:
                    return True
        return False
