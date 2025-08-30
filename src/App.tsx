import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save, open } from "@tauri-apps/plugin-dialog";
import Logo from "@/components/Logo";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { listen } from "@tauri-apps/api/event";
import { EyeIcon, EyeOffIcon, CopyIcon } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";

import { Toaster, toast } from "sonner";
import { Combobox } from "@/components/ui/combobox";

import "./style.css";

const appWindow = WebviewWindow.getCurrent();

function App() {
  const [password, setPassword] = useState("");
  const [droppedFilePaths, setDroppedFilePaths] = useState<string[]>([]);
  const [progress, setProgress] = useState(0);
  const [isEncrypting, setIsEncrypting] = useState(false);
  const [showPassword, setShowPassword] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [successMessage, setSuccessMessage] = useState<string | null>(null);
  const [zipOutputPath, setZipOutputPath] = useState<string | null>(null);

  useEffect(() => {
    const unlistenProgress = listen<number>("encryption_progress", (event) => {
      setProgress(event.payload);
    });

    const unlistenDrop = appWindow.onDragDropEvent((event) => {
      if (event.payload.type === "drop") {
        setDroppedFilePaths(event.payload.paths);
        setErrorMessage(null); // Clear error on new file drop
      }
    });

    return () => {
      unlistenProgress.then((f) => f());
      unlistenDrop.then((f) => f());
    };
  }, []);

  const handleCancel = async () => {
    setIsEncrypting(false);
    setProgress(0);
    setErrorMessage(null);
    setSuccessMessage(null);
    setDroppedFilePaths([]);
    setZipOutputPath(null);
    await invoke("cancel_encryption");
    toast.info("Chiffrement annulé.", { duration: 2000 });
  };

    const generatePassword = async () => {
    try {
      const newPassword = await invoke<string>("generate_password");
      setPassword(newPassword);
      setErrorMessage(null);
    } catch (error) {
      console.error("Failed to generate password:", error);
      setErrorMessage("Failed to generate password from backend.");
    }
  };

  const copyPassword = () => {
    if (password) {
      navigator.clipboard
        .writeText(password)
        .then(() => {
          toast.success("Mot de passe copié !", { duration: 2000 });
        })
        .catch((err) => {
          console.error("Failed to copy password: ", err);
          toast.error("Erreur lors de la copie.", { duration: 2000 });
        });
    }
  };

  const selectFiles = async () => {
    try {
      const selected = await open({ multiple: true });
      let newFiles: string[] = [];
      if (Array.isArray(selected)) {
        newFiles = selected;
      } else if (selected) {
        newFiles = [selected];
      }

      setDroppedFilePaths((prevPaths) => {
        const uniquePaths = new Set([...prevPaths, ...newFiles]);
        return Array.from(uniquePaths);
      });
      setErrorMessage(null); // Clear error on new file selection
    } catch (error) {
      console.error("Failed to select files:", error);
    }
  };

  const removeFile = (pathToRemove: string) => {
    setDroppedFilePaths((prevPaths) =>
      prevPaths.filter((path) => path !== pathToRemove)
    );
  };

  const clearFiles = () => {
    setDroppedFilePaths([]);
    setErrorMessage(null);
    setSuccessMessage(null);
  };

    const [encryptionMethod, setEncryptionMethod] = useState("Aes256");

  const encryptFiles = async () => {
    if (droppedFilePaths.length === 0) {
      setErrorMessage(
        "Veuillez sélectionner ou glisser-déposer des fichiers d'abord."
      );
      return;
    }

    if (!password) {
      setErrorMessage("Veuillez entrer ou générer un mot de passe.");
      return;
    }

    setErrorMessage(null);
    setIsEncrypting(true);
    setProgress(0);

    try {
      const filters = encryptionMethod === 'SevenZip' 
        ? [{ name: '7-Zip Archive', extensions: ['7z'] }]
        : [{ name: 'Zip Archive', extensions: ['zip'] }];

      const savePath = await save({
        filters,
      });

      if (savePath) {
        const result = await invoke("encrypt-files", {
          filePaths: droppedFilePaths,
          outputPath: savePath,
          password,
          encryptionMethod,
        });

        if (result === "Encryption cancelled by user.") {
          toast.info("Chiffrement de l'archive annulé !", { duration: 2000 });
        } else {
          setSuccessMessage(`Archive exportée vers ${savePath}`);
          setZipOutputPath(savePath);
        }
      } else {
        toast.info("Chiffrement annulé par l'utilisateur.", { duration: 2000 });
      }
    } catch (error) {
      console.error("Encryption failed:", error);
      if (error === "Encryption cancelled by user.") {
        // Do nothing, toast is already shown
      } else {
        setErrorMessage(`Échec du chiffrement : ${error}`);
      }
    } finally {
      setIsEncrypting(false);
      setProgress(0);
      setDroppedFilePaths([]);
    }
  };

  return (
    <div className="min-h-screen flex items-start justify-center p-4 font-sans">
      <Card className="w-full max-w-2xl mx-auto flex flex-col my-auto">
        <CardHeader>
          <CardTitle className="flex justify-center">
            <Logo className="w-full" />
          </CardTitle>
          <CardDescription className="text-lg text-center mb-4">
            Chiffrez vos fichiers en toute simplicité.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col flex-grow">
          <div className="grid w-full items-center gap-6">
            <div
              className="flex flex-col items-center justify-center p-6 border-2 border-dashed rounded-lg cursor-pointer min-h-[8rem] py-8"
              onClick={selectFiles}
            >
              {droppedFilePaths.length > 0 ? (
                <div className="w-full">
                  <div className="flex justify-between items-center mb-2">
                    <p className="text-sm font-medium">
                      Fichier(s) sélectionné(s) :
                    </p>
                    <Button variant="ghost" size="sm" onClick={clearFiles}>
                      Tout effacer
                    </Button>
                  </div>
                  <ul className="list-none p-0 m-0 space-y-1 max-h-32 overflow-y-auto scroll-fade">
                    {droppedFilePaths.map((path, index) => (
                      <li
                        key={index}
                        className="flex items-center justify-between text-xs bg-secondary p-1.5 rounded-md"
                      >
                        <span>{path.split("/").pop()}</span>
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={(e) => {
                            e.stopPropagation();
                            removeFile(path);
                          }}
                        >
                          <svg
                            xmlns="http://www.w3.org/2000/svg"
                            width="16"
                            height="16"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            strokeWidth="2"
                            strokeLinecap="round"
                            strokeLinejoin="round"
                            className="lucide lucide-x"
                          >
                            <path d="M18 6 6 18" />
                            <path d="m6 6 12 12" />
                          </svg>
                        </Button>
                      </li>
                    ))}
                  </ul>
                </div>
              ) : (
                <p>
                  Glissez-déposez des fichiers ici, ou cliquez pour sélectionner
                </p>
              )}
            </div>
                        <div className="flex flex-col gap-4">
              <div className="flex items-center space-x-2">
                <Input
                  id="password"
                  placeholder="Entrez votre mot de passe"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  type={showPassword ? "text" : "password"}
                  className="flex-grow"
                />
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => setShowPassword(!showPassword)}
                  aria-label={
                    showPassword
                      ? "Masquer le mot de passe"
                      : "Afficher le mot de passe"
                  }
                >
                  {showPassword ? (
                    <EyeOffIcon className="h-4 w-4" />
                  ) : (
                    <EyeIcon className="h-4 w-4" />
                  )}
                </Button>
                <Button
                  variant="outline"
                  onClick={copyPassword}
                  disabled={!password}
                >
                  <CopyIcon className="h-4 w-4 mr-2" /> Copier
              </Button>
            </div>
              <Button variant="outline" onClick={generatePassword}>
                Générer un mot de passe
              </Button>
            </div>
            <div className="flex flex-col gap-2">
              <p className="text-sm font-medium">Méthode de chiffrement :</p>
              <Combobox
                value={encryptionMethod}
                onChange={setEncryptionMethod}
                options={[
                  {
                    value: "Aes256",
                    label: "AES-256 (Recommandé)",
                    description: "Chiffrement fort. Natif macOS. Requiert 7-Zip sur Windows/Linux.",
                  },
                  {
                    value: "CryptoZip",
                    label: "CryptoZip (Compatible)",
                    description: "Chiffrement basique. Compatibilité native Windows/macOS/Linux.",
                  },
                  {
                    value: "SevenZip",
                    label: "7-Zip (Fichiers masqués)",
                    description: "Chiffrement fort, masque les noms de fichiers. Requiert 7-Zip sur Windows/macOS/Linux.",
                  },
                ]}
              />
            </div>
          </div>
        </CardContent>
        <CardFooter className="flex flex-col space-y-4">
          {errorMessage && (
            <p className="text-destructive text-sm text-center w-full">
              {errorMessage}
            </p>
          )}
          <Button
            onClick={isEncrypting ? handleCancel : encryptFiles}
            disabled={droppedFilePaths.length === 0 || !password}
            className="w-full"
            variant={isEncrypting ? "destructive" : "default"}
          >
            {isEncrypting ? "Annuler" : "Chiffrer"}
          </Button>
          {isEncrypting && (
            <div className="w-full">
              <Progress value={progress} className="w-full" />
              <p className="text-sm text-center mt-2">{progress}%</p>
            </div>
          )}
          {successMessage && (
            <Alert className="w-full flex flex-col items-center text-center">
              <AlertDescription className="mb-4">
                Archive exportée vers :{" "}
                <span className="font-mono text-sm break-all">
                  {zipOutputPath}
                </span>
              </AlertDescription>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  console.log("Close button clicked");
                  setSuccessMessage(null);
                  setZipOutputPath(null);
                  setPassword(""); // Clear the password
                }}
                className="p-2"
              >
                Fermer
              </Button>
            </Alert>
          )}
        </CardFooter>
      </Card>
      <Toaster position="bottom-center" richColors />
    </div>
  );
}

export default App;
