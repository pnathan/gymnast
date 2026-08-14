#!/usr/bin/env lamedh
;;; Drive actual synthesis for the persistence node.
;;; Produces 3 independent candidates via gymnast-run-node
;;; with the Claude subprocess provider.
;;; Run from repo root: .tools/bin/lamedh scripts/synthesize-persistence.lisp

(include "../src/gymnast.lisp")

(defun find-persistence-node (plan)
  (let ((nodes (gymnast-plan-field plan 'nodes)))
    (car (filter
        (lambda (n)
          (equal (gymnast-plan-node-id n) "todo-spec/plan/persistence"))
        nodes))))

(defun write-candidate-result (dir index result)
  (let ((filename (concat dir "/candidate-" (princ-to-string index) ".sexpr")))
    (write-file filename
      (concat (prin1-to-string result) (code-char 10)))
    (princ (concat "Wrote " filename))
    (terpri)))

(defun run-synthesis ()
  (load-file "examples/todo.lisp")
  (let* ((surface (eval (intern "todo-spec")))
      (ir (gymnast-elaborate surface))
      (plan (gymnast-plan ir))
      (node (find-persistence-node plan))
      (provider (gymnast-make-claude-provider))
      (max-attempts 3)
      (output-dir "build/synthesis"))
    (if (null node)
      (progn
        (princ "ERROR: persistence node not found in plan")
        (terpri)
        (exit 1))
      (progn
        (princ "Found persistence node. Running 3 synthesis passes...")
        (terpri)
        (create-directory output-dir)
        (let ((results nil))
          (dotimes (i 3)
            (let ((run-num (+ i 1)))
              (princ (concat "=== Synthesis pass " (princ-to-string run-num) " of 3 ==="))
              (terpri)
              (let ((result (gymnast-run-node ir plan node provider max-attempts)))
                (write-candidate-result output-dir run-num result)
                (princ (concat "Status: "
                    (princ-to-string
                      (gymnast-run-result-field result 'status))))
                (terpri)
                (setq results (cons result results)))))
          (princ "=== All 3 synthesis passes complete ===")
          (terpri)
          results)))))

(run-synthesis)
