#!/usr/bin/env python3
import json
import sys
import os

def main():
    if len(sys.argv) > 1:
        file_path = sys.argv[1]
    else:
        file_path = "mythrax-core/bench_data/results_full500.jsonl"

    if not os.path.exists(file_path):
        print(f"Error: File '{file_path}' does not exist.")
        sys.exit(1)

    questions = []
    with open(file_path, "r") as f:
        for line in f:
            data = json.loads(line)
            if "question_id" in data:
                questions.append(data)

    total = len(questions)
    if total == 0:
        print("No questions found in results file.")
        sys.exit(0)

    sum_any_turn_5 = sum(q.get("recall_any_turn_at5", 0.0) for q in questions)
    sum_all_turn_5 = sum(q.get("recall_all_turn_at5", 0.0) for q in questions)
    sum_ndcg_10 = sum(q.get("ndcg_turn_at10", 0.0) for q in questions)
    sum_any_turn_10 = sum(q.get("recall_any_turn_at10", 0.0) for q in questions)
    sum_any_sess_5 = sum(q.get("recall_any_session_at5", 0.0) for q in questions)
    sum_all_sess_5 = sum(q.get("recall_all_session_at5", 0.0) for q in questions)

    print(f"Results File: {file_path}")
    print(f"Total Questions: {total}")
    print(f"Recall_Any@5 (turn): {sum_any_turn_5 / total:.4f}")
    print(f"Recall_All@5 (turn): {sum_all_turn_5 / total:.4f}")
    print(f"nDCG@10 (turn): {sum_ndcg_10 / total:.4f}")
    print(f"Recall_Any@10 (turn): {sum_any_turn_10 / total:.4f}")
    print(f"Recall_Any@5 (session): {sum_any_sess_5 / total:.4f}")
    print(f"Recall_All@5 (session): {sum_all_sess_5 / total:.4f}")

    print("\nPer-Type Metrics:")
    types = set(q.get("question_type") for q in questions if "question_type" in q)
    for t in sorted(types):
        q_type = [q for q in questions if q.get("question_type") == t]
        count = len(q_type)
        r_any_5 = sum(q.get("recall_any_turn_at5", 0.0) for q in q_type) / count
        r_any_10 = sum(q.get("recall_any_turn_at10", 0.0) for q in q_type) / count
        ndcg = sum(q.get("ndcg_turn_at10", 0.0) for q in q_type) / count
        print(f"  - {t:<28} (n={count:<3}): R@5={r_any_5:.4f}, R@10={r_any_10:.4f}, nDCG@10={ndcg:.4f}")

if __name__ == "__main__":
    main()
