#!/usr/bin/env bash
# Checks that a set of split sqllogictest lists partitions a full-tree list:
# every file is covered by exactly one group, none dropped, none run twice.
#
# usage: sql_logic_coverage_check.sh FULL_TREE_TESTLIST GROUP_TESTLIST...

set -euo pipefail
export LC_ALL=C

if [ "$#" -lt 2 ]; then
  echo "usage: $0 FULL_TREE_TESTLIST GROUP_TESTLIST..." >&2
  exit 1
fi

oracle=$1
shift

work_dir=$(mktemp -d)
trap 'rm -rf "$work_dir"' EXIT

sort "$oracle" > "$work_dir/oracle"
cat "$@" | sort > "$work_dir/union"
sort -u "$work_dir/union" > "$work_dir/union_distinct"

status=0

missing=$(comm -23 "$work_dir/oracle" "$work_dir/union_distinct")
if [ -n "$missing" ]; then
  echo "sqllogictest files with no owning group:" >&2
  echo "$missing" >&2
  status=1
fi

extra=$(comm -13 "$work_dir/oracle" "$work_dir/union_distinct")
if [ -n "$extra" ]; then
  echo "sqllogictest files in a group list but not in the full-tree list:" >&2
  echo "$extra" >&2
  status=1
fi

duplicates=$(uniq -d "$work_dir/union")
if [ -n "$duplicates" ]; then
  echo "sqllogictest files listed by more than one group:" >&2
  echo "$duplicates" >&2
  status=1
fi

exit $status
