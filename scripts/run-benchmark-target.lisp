#!/usr/bin/env lamedh
;;; Run synthesis trials for a single target.  Called by benchmark.sh.
;;; Args: spec-file spec-name label trial-count max-attempts

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

(defun run-single-trial (ir plan node provider max-attempts)
  (let* ((result (gymnast-run-node ir plan node provider max-attempts))
      (status (gymnast-run-result-field result 'status))
      (attempts (gymnast-run-result-field result 'attempts))
      (count (length attempts)))
    (list 'trial
      (list 'status status)
      (list 'attempt-count count))))

(defun run-trials (spec-file spec-name label n max-attempts)
  (load-file spec-file)
  (let* ((surface (eval (intern spec-name)))
      (ir (gymnast-elaborate surface))
      (plan (gymnast-plan ir))
      (node (find-persistence-node plan))
      (provider (gymnast-make-claude-provider))
      (i 0))
    (while (< i n)
      (let* ((trial (run-single-trial ir plan node provider max-attempts))
          (status (gymnast-assoc-value 'status (cdr trial)))
          (count (gymnast-assoc-value 'attempt-count (cdr trial))))
        (princ (concat label " trial "
            (princ-to-string (+ i 1)) "/" (princ-to-string n)
            ": " (princ-to-string status)
            " (" (princ-to-string count) " attempt"
            (if (> count 1) "s" "") ")"))
        (terpri))
      (setq i (+ i 1)))))

(if (< (length *ARGV*) 5)
  (progn
    (princ "Usage: run-benchmark-target.lisp SPEC-FILE SPEC-NAME LABEL TRIAL-COUNT MAX-ATTEMPTS")
    (terpri)
    (exit 1))
  (run-trials
    (nth 0 *ARGV*) (nth 1 *ARGV*) (nth 2 *ARGV*)
    (parse-integer (nth 3 *ARGV*))
    (parse-integer (nth 4 *ARGV*))))
