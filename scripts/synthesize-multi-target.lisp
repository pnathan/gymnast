#!/usr/bin/env lamedh
;;; Multi-target synthesis: runs persistence node synthesis across
;;; Ruby, Go, and Java Spring targets to validate target-language
;;; generalization.
;;; Run from repo root: .tools/bin/lamedh scripts/synthesize-multi-target.lisp

(include "../src/gymnast.lisp")

(defun ends-with-p (str suffix)
  (let ((slen (length suffix)) (tlen (length str)))
    (and (>= tlen slen)
      (equal (substring str (- tlen slen) tlen) suffix))))

(defun find-persistence-node (plan)
  (let ((nodes (gymnast-plan-field plan 'nodes)))
    (car (filter
        (lambda (n)
          (ends-with-p (gymnast-plan-node-id n) "/plan/persistence"))
        nodes))))

(defun write-result (dir label result)
  (let ((filename (concat dir "/" label ".sexpr")))
    (write-file filename
      (concat (prin1-to-string result) (code-char 10)))
    (princ (concat "  Wrote " filename))
    (terpri)))

(defun synthesize-target (spec-file spec-name label output-dir)
  (princ (concat "=== " label " ==="))
  (terpri)
  (load-file spec-file)
  (let* ((surface (eval (intern spec-name)))
      (ir (gymnast-elaborate surface))
      (plan (gymnast-plan ir))
      (node (find-persistence-node plan))
      (provider (gymnast-make-claude-provider))
      (max-attempts 3))
    (if (null node)
      (progn
        (princ (concat "  ERROR: persistence node not found for " label))
        (terpri))
      (let ((result (gymnast-run-node ir plan node provider max-attempts)))
        (write-result output-dir label result)
        (let ((status (gymnast-run-result-field result 'status)))
          (princ (concat "  Status: " (princ-to-string status)))
          (terpri)
          result)))))

(defun run-multi-target ()
  (let ((output-dir "build/synthesis-multi"))
    (create-directory output-dir)
    (princ "Multi-target synthesis: Ruby, Go, Java Spring")
    (terpri)
    (terpri)
    (synthesize-target
      "examples/todo.lisp" "todo-spec" "ruby" output-dir)
    (terpri)
    (synthesize-target
      "examples/todo-go.lisp" "todo-go-spec" "go" output-dir)
    (terpri)
    (synthesize-target
      "examples/todo-java.lisp" "todo-java-spec" "java-spring" output-dir)
    (terpri)
    (princ "=== All targets complete ===")))

(run-multi-target)
