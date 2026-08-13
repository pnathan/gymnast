#!/usr/bin/env lamedh
;;; Validate enriched prompts produce reliable first-attempt synthesis.

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

(defun run-target (spec-file spec-name label)
  (load-file spec-file)
  (let* ((surface (eval (intern spec-name)))
      (ir (gymnast-elaborate surface))
      (plan (gymnast-plan ir))
      (node (find-persistence-node plan))
      (provider (gymnast-make-claude-provider))
      (result (gymnast-run-node ir plan node provider 3))
      (status (gymnast-run-result-field result 'status))
      (attempts (gymnast-run-result-field result 'attempts))
      (count (length attempts))
      (out-file (concat "build/synthesis-enriched/" label ".sexpr")))
    (shell "mkdir -p build/synthesis-enriched")
    (write-file out-file
      (concat (prin1-to-string result) (code-char 10)))
    (princ (concat label ": " (princ-to-string status)
        " (" (princ-to-string count) " attempt"
        (if (> count 1) "s" "") ")"))
    (terpri)
    result))

(princ "Running enriched prompt synthesis...")
(terpri)
(run-target "examples/todo.lisp" "todo-spec" "ruby")
(run-target "examples/todo-go.lisp" "todo-go-spec" "go")
(run-target "examples/todo-java.lisp" "todo-java-spec" "java-spring")
