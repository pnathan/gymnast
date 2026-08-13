#!/usr/bin/env lamedh
;;; Run N synthesis trials per target and collate results.

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
      (results nil)
      (i 0))
    (while (< i n)
      (let* ((trial (run-single-trial
              ir plan node provider max-attempts))
          (status (gymnast-assoc-value 'status (cdr trial)))
          (count (gymnast-assoc-value 'attempt-count (cdr trial))))
        (princ (concat "  " label " trial "
            (princ-to-string (+ i 1)) "/" (princ-to-string n)
            ": " (princ-to-string status)
            " (" (princ-to-string count) " attempt"
            (if (> count 1) "s" "") ")"))
        (terpri)
        (setq results (append results (list trial))))
      (setq i (+ i 1)))
    (list 'target-results
      (list 'label label)
      (list 'trials results))))

(defun count-by-status (trials status)
  (length (filter
      (lambda (t)
        (equal (gymnast-assoc-value 'status (cdr t)) status))
      trials)))

(defun sum-attempts (trials)
  (if (null trials) 0
    (+ (gymnast-assoc-value 'attempt-count (cdr (car trials)))
      (sum-attempts (cdr trials)))))

(defun print-summary (target-result)
  (let* ((label (gymnast-assoc-value 'label (cdr target-result)))
      (trials (gymnast-assoc-value 'trials (cdr target-result)))
      (n (length trials))
      (succeeded (count-by-status trials 'succeeded))
      (exhausted (count-by-status trials 'exhausted))
      (first-attempt (length (filter
            (lambda (t)
              (and
                (equal (gymnast-assoc-value 'status (cdr t))
                  'succeeded)
                (= (gymnast-assoc-value 'attempt-count (cdr t))
                  1)))
            trials)))
      (total-attempts (sum-attempts trials)))
    (princ (concat "  " label ":"
        " " (princ-to-string succeeded) "/" (princ-to-string n)
        " passed"
        ", " (princ-to-string first-attempt)
        " first-attempt"
        ", " (princ-to-string exhausted) " exhausted"
        ", avg attempts "
        (princ-to-string
          (if (> succeeded 0)
            (let* ((success-trials (filter
                    (lambda (t)
                      (equal (gymnast-assoc-value 'status (cdr t))
                        'succeeded))
                    trials))
                (success-attempts (sum-attempts success-trials)))
              (/ success-attempts succeeded))
            0))
        " (on successes)"))
    (terpri)))

(defun write-results (all-results out-file)
  (shell "mkdir -p build/trials")
  (write-file out-file
    (concat (prin1-to-string all-results) (code-char 10))))

;;; Main.

(def $trial-count 8)
(def $max-attempts 3)

(princ (concat "Synthesis trials: " (princ-to-string $trial-count)
    " trials per target, max " (princ-to-string $max-attempts)
    " attempts each"))
(terpri)
(terpri)

(princ "Ruby/Rails:")
(terpri)
(def $ruby-results
  (run-trials "examples/todo.lisp" "todo-spec"
    "ruby" $trial-count $max-attempts))

(terpri)
(princ "Go/stdlib:")
(terpri)
(def $go-results
  (run-trials "examples/todo-go.lisp" "todo-go-spec"
    "go" $trial-count $max-attempts))

(terpri)
(princ "Java/Spring:")
(terpri)
(def $java-results
  (run-trials "examples/todo-java.lisp" "todo-java-spec"
    "java" $trial-count $max-attempts))

(terpri)
(princ "Python/Django:")
(terpri)
(def $python-results
  (run-trials "examples/todo-python.lisp" "todo-python-spec"
    "python" $trial-count $max-attempts))

(def $all-results
  (list $ruby-results $go-results $java-results $python-results))

(write-results $all-results "build/trials/results.sexpr")

(terpri)
(princ "=== SUMMARY ===")
(terpri)
(print-summary $ruby-results)
(print-summary $go-results)
(print-summary $java-results)
(print-summary $python-results)
