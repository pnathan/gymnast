#!/usr/bin/env lamedh
;;; Gymnast command-line entrypoint.  Run through bin/gymnast.

(include "gymnast.lisp")

(defun gymnast-cli-usage ()
  (princ "usage: gymnast <check|ir|plan|prompts|compile> SPEC-FILE BINDING [OUTPUT-DIR]")
  (terpri))

(defun gymnast-cli-print (value)
  (prin1 value)
  (terpri))

(defun gymnast-cli-load-spec (path binding)
  (load-file path)
  (eval (intern binding)))

(defun gymnast-cli-write-value (directory filename value)
  (write-file (concat directory "/" filename)
              (concat (prin1-to-string value) (code-char 10))))

(defun gymnast-cli-write-compilation (directory compilation)
  (create-directory directory)
  (gymnast-cli-write-value directory "compilation.sexpr" compilation)
  (gymnast-cli-write-value directory "ir.sexpr"
                           (gymnast-compilation-field compilation 'ir))
  (gymnast-cli-write-value directory "plan.sexpr"
                           (gymnast-compilation-field compilation 'plan))
  (gymnast-cli-write-value directory "prompts.sexpr"
                           (gymnast-compilation-field compilation 'prompts))
  (gymnast-cli-print
    (list 'wrote directory
          (gymnast-compilation-field compilation 'fingerprint))))

(defun gymnast-cli-main (args)
  (if (< (length args) 3)
      (progn (gymnast-cli-usage) (exit 2))
      (let* ((command (car args))
             (path (cadr args))
             (binding (caddr args))
             (surface (gymnast-cli-load-spec path binding))
             (ir (gymnast-elaborate surface)))
        (cond
          ((equal command "check")
           (gymnast-cli-print (gymnast-ir-field ir 'diagnostics))
           (exit (if (gymnast-has-errors-p
                       (gymnast-ir-field ir 'diagnostics)) 1 0)))
          ((equal command "ir")
           (gymnast-cli-print (gymnast-assert-valid-ir ir)))
          ((equal command "plan")
           (gymnast-cli-print (gymnast-plan ir)))
          ((equal command "prompts")
           (let ((plan (gymnast-plan ir)))
             (gymnast-cli-print (gymnast-compile-prompts ir plan))))
          ((equal command "compile")
           (if (< (length args) 4)
               (progn (gymnast-cli-usage) (exit 2))
               (gymnast-cli-write-compilation
                 (cadddr args) (gymnast-compile surface))))
          (t
           (gymnast-cli-usage)
           (exit 2))))))

(gymnast-cli-main *argv*)
