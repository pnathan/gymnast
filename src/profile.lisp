;;; Versioned semantic profiles.
;;;
;;; A profile is a reusable, parameterized module of declarations
;;; analogous to a hardware IP core.  Profiles are registered by name
;;; and version, instantiated with typed arguments during elaboration,
;;; and expanded purely into ordinary kernel declarations.

(defun gymnast-profile-prop-key (version)
  (concat "gymnast.profile/"
    (if (stringp version) version (princ-to-string version))))

(defun gymnast-define-profile (name version params generator)
  (putp name (gymnast-profile-prop-key version)
    (list 'profile
      (list 'name name)
      (list 'version version)
      (list 'params params)
      (list 'generator generator)))
  name)

(defun gymnast-lookup-profile (name version)
  (if version
    (getp name (gymnast-profile-prop-key version))
    nil))

(defun gymnast-profile-field (profile key)
  (gymnast-assoc-value key (cdr profile)))

;;; Argument parsing and parameter validation.
;;;
;;; Parameters: ((keyword type default) ...) where default = REQUIRED
;;; for mandatory decisions.  Arguments arrive as a flat plist from
;;; the use-profile macro expansion.

(defun gymnast-plist-to-alist (plist)
  (if (or (null plist) (null (cdr plist)))
    nil
    (cons (list (car plist) (cadr plist))
      (gymnast-plist-to-alist (cdr (cdr plist))))))

(defun gymnast-validate-profile-params-rec (params args subject)
  (if (null params)
    (list nil nil)
    (let* ((param (car params))
        (key (car param))
        (param-default (caddr param))
        (provided (assoc key args))
        (rest (gymnast-validate-profile-params-rec
            (cdr params) args subject)))
      (if (and (not provided) (equal param-default 'required))
        (list (car rest)
          (cons (gymnast-diagnostic 'error 'missing-profile-decision
              subject
              (concat "profile requires a decision for "
                (princ-to-string key))
              key)
            (cadr rest)))
        (list
          (cons (list key (if provided (cadr provided) param-default))
            (car rest))
          (cadr rest))))))

;;; Profile identification on surface declarations.

(defun gymnast-is-profile-import-p (surface)
  (and (gymnast-surface-p surface)
    (equal (gymnast-surface-kind surface) 'import)
    (let ((ops (gymnast-surface-operands surface)))
      (equal (gymnast-keyword-value ops ':authority) 'authoritative))))

;;; Profile expansion.  Returns (list generated-surfaces diagnostics).

(defun gymnast-expand-profile-import (module-name surface)
  (let* ((profile-name (gymnast-surface-name surface))
      (ops (gymnast-surface-operands surface))
      (version (gymnast-keyword-value ops ':version))
      (profile (gymnast-lookup-profile profile-name version)))
    (if (not profile)
      (list nil nil)
      (let* ((arguments (gymnast-keyword-value ops ':arguments))
          (subject (concat (gymnast-symbol-string module-name)
              "/import/" (gymnast-symbol-string profile-name)))
          (params (gymnast-profile-field profile 'params))
          (arg-alist (gymnast-plist-to-alist arguments))
          (validated (gymnast-validate-profile-params-rec
              params arg-alist subject))
          (resolved-args (car validated))
          (diagnostics (cadr validated))
          (generator (gymnast-profile-field profile 'generator)))
        (if (gymnast-has-errors-p diagnostics)
          (list nil diagnostics)
          (let ((generated (funcall generator resolved-args)))
            (list generated diagnostics)))))))
