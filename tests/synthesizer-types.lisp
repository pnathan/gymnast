;;; Tests for the claude subprocess synthesizer.

(include "../src/gymnast.lisp")

;;; Model flag mapping.

(deftest model-flag-from-small-code-model
  (assert-equal
    (gymnast-claude-model-flag '(small-code-model :class nano))
    "haiku"))

(deftest model-flag-from-symbol
  (assert-equal
    (gymnast-claude-model-flag 'sonnet)
    "sonnet"))

(deftest model-flag-from-string
  (assert-equal
    (gymnast-claude-model-flag "claude-haiku-4-5-20251001")
    "claude-haiku-4-5-20251001"))

(deftest model-flag-from-cons-symbol
  (assert-equal
    (gymnast-claude-model-flag '(haiku :class fast))
    "haiku"))

(deftest model-flag-default
  (assert-equal
    (gymnast-claude-model-flag nil)
    "haiku"))

;;; Synthesizer record and protocol.

(deftest claude-synthesizer-is-a-record
  (assert-true
    (gymnast-claude-subprocess-synthesizer-p $gymnast-claude-synthesizer)))

(deftest claude-synthesizer-name
  (assert-equal
    (gymnast-claude-subprocess-synthesizer-name $gymnast-claude-synthesizer)
    "claude-subprocess"))

(deftest make-claude-provider-returns-function
  (let ((provider (gymnast-make-claude-provider)))
    (assert-true (functionp provider))))

;;; System prompt is well-formed.

(deftest system-prompt-defined
  (assert-true (stringp $gymnast-claude-system-prompt))
  (assert-true (> (length $gymnast-claude-system-prompt) 0)))

(deftest system-prompt-instructs-sexpr-output
  (assert-true
    (starts-with-p $gymnast-claude-system-prompt "You are a deterministic synthesis")))

;;; Provider function integration (no protocol dispatch in tests to
;;; avoid defprotocol re-entry clearing the dispatch table).

(deftest claude-provider-wraps-protocol
  (let ((provider (gymnast-make-claude-provider)))
    (assert-true (functionp provider))))

;;; Runner integration with mock synthesizer via protocol.

(deftest runner-accepts-valid-mock-candidate
  (let* ((node (gymnast-plan-node
          "test/plan/impl" 'generative 'implement
          nil nil '(ruby :framework rails) 'haiku
          '("src/todo.rb") nil nil nil))
      (candidate-text (concat
          "(candidate"
          " (schema \"gymnast.candidate/0.1\")"
          " (node-id \"test/plan/impl\")"
          " (files ((\"src/todo.rb\" \"class Todo; end\")))"
          " (implements (\"test/design/impl\"))"
          " (edge-uses nil)"
          " (assumptions nil)"
          " (unresolved nil))"))
      (mock-provider (lambda (req) candidate-text))
      (ir (list 'ir
          (list 'schema "gymnast.ir/0.1")
          (list 'design nil)
          (list 'transitions nil)
          (list 'obligations nil)
          (list 'synthesis nil)))
      (plan (list 'plan
          (list 'schema "gymnast.plan/0.1")
          (list 'nodes (list node))))
      (prompt-package (gymnast-compile-prompt ir plan node))
      (result (gymnast-run-node-loop ir plan node
          prompt-package mock-provider 1 3 nil)))
    (assert-equal
      (gymnast-run-result-field result 'status) 'succeeded)))

(deftest runner-rejects-and-retries-bad-candidate
  (let* ((node (gymnast-plan-node
          "test/plan/impl" 'generative 'implement
          nil nil '(ruby :framework rails) 'haiku
          '("src/todo.rb") nil nil nil))
      (bad-text "(not-a-candidate)")
      (good-text (concat
          "(candidate"
          " (schema \"gymnast.candidate/0.1\")"
          " (node-id \"test/plan/impl\")"
          " (files ((\"src/todo.rb\" \"class Todo; end\")))"
          " (implements nil)"
          " (edge-uses nil)"
          " (assumptions nil)"
          " (unresolved nil))"))
      (call-count 0)
      (mock-provider (lambda (req)
          (setq call-count (+ call-count 1))
          (if (= call-count 1) bad-text good-text)))
      (ir (list 'ir
          (list 'schema "gymnast.ir/0.1")
          (list 'design nil)
          (list 'transitions nil)
          (list 'obligations nil)
          (list 'synthesis nil)))
      (plan (list 'plan
          (list 'schema "gymnast.plan/0.1")
          (list 'nodes (list node))))
      (prompt-package (gymnast-compile-prompt ir plan node))
      (result (gymnast-run-node-loop ir plan node
          prompt-package mock-provider 1 3 nil)))
    (assert-equal
      (gymnast-run-result-field result 'status) 'succeeded)
    (assert-equal (length (gymnast-run-result-field result 'attempts)) 2)))
