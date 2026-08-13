#!/usr/bin/env lamedh
;;; Re-run Java Spring synthesis with improved diagnostic.

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

(load-file "examples/todo-java.lisp")
(let* ((surface (eval (intern "todo-java-spec")))
    (ir (gymnast-elaborate surface))
    (plan (gymnast-plan ir))
    (node (find-persistence-node plan))
    (provider (gymnast-make-claude-provider))
    (result (gymnast-run-node ir plan node provider 3)))
  (write-file "build/synthesis-multi/java-spring.sexpr"
    (concat (prin1-to-string result) (code-char 10)))
  (princ (concat "Status: "
      (princ-to-string (gymnast-run-result-field result 'status))))
  (terpri))
