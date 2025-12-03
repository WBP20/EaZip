import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save, open } from "@tauri-apps/plugin-dialog";
import Logo from "@/components/Logo";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { listen } from "@tauri-apps/api/event";
import { EyeIcon, EyeOffIcon, CopyIcon, FileIcon, FolderIcon } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Toaster } from '@/components/ui/sonner';
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

import { toast } from "@/components/ui/sonner";
import { Combobox } from "@/components/ui/combobox";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";



const appWindow = WebviewWindow.getCurrent();

function App() {
  const [password, setPassword] = useState("");
  const [droppedFiles, setDroppedFiles] = useState<{ path: string; isDir: boolean }[]>([]);
  const [progress, setProgress] = useState(0);
  const [isEncrypting, setIsEncrypting] = useState(false);
  const [showPassword, setShowPassword] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [successMessage, setSuccessMessage] = useState<string | null>(null);
  const [zipOutputPath, setZipOutputPath] = useState<string | null>(null);
  const [mode, setMode] = useState<"encrypt" | "decrypt">("encrypt");

  useEffect(() => {
    const unlistenProgress = listen<number>("encryption_progress", (event) => {
      setProgress(event.payload);
    });

    const unlistenDrop = appWindow.onDragDropEvent(async (event) => {
      if (event.payload.type === "drop") {
        const paths = event.payload.paths;
        try {
          const metadata = await invoke<{ path: string; isDir: boolean }[]>("get_file_metadata", { paths });
          setDroppedFiles(metadata);
          setErrorMessage(null);
        } catch (error) {
          console.error("Failed to get file metadata:", error);
          // Fallback to assuming files if metadata fails, or handle error appropriately
          setDroppedFiles(paths.map(p => ({ path: p, isDir: false })));
        }
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
    setDroppedFiles([]);
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

      const newFileObjects = newFiles.map(path => ({ path, isDir: false }));

      setDroppedFiles((prevFiles) => {
        // Filter out duplicates based on path
        const existingPaths = new Set(prevFiles.map(f => f.path));
        const uniqueNewFiles = newFileObjects.filter(f => !existingPaths.has(f.path));
        return [...prevFiles, ...uniqueNewFiles];
      });
      setErrorMessage(null); // Clear error on new file selection
    } catch (error) {
      console.error("Failed to select files:", error);
    }
  };

  const selectFolder = async () => {
    try {
      const selected = await open({ directory: true, multiple: true });
      let newFiles: string[] = [];
      if (Array.isArray(selected)) {
        newFiles = selected;
      } else if (selected) {
        newFiles = [selected];
      }

      const newFileObjects = newFiles.map(path => ({ path, isDir: true }));

      setDroppedFiles((prevFiles) => {
        const existingPaths = new Set(prevFiles.map(f => f.path));
        const uniqueNewFiles = newFileObjects.filter(f => !existingPaths.has(f.path));
        return [...prevFiles, ...uniqueNewFiles];
      });
      setErrorMessage(null);
    } catch (error) {
      console.error("Failed to select folder:", error);
    }
  };

  const removeFile = (pathToRemove: string) => {
    setDroppedFiles((prevFiles) =>
      prevFiles.filter((file) => file.path !== pathToRemove)
    );
  };

  const clearFiles = () => {
    setDroppedFiles([]);
    setErrorMessage(null);
    setSuccessMessage(null);
  };

  const [encryptionMethod, setEncryptionMethod] = useState("Aes256");

  const encryptFiles = async () => {
    if (droppedFiles.length === 0) {
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

      let defaultPath = "archive";
      if (droppedFiles.length > 0) {
        const firstFile = droppedFiles[0];
        // Extract filename from path
        let baseName = firstFile.path.split(/[/\\]/).pop() || "archive";
        // Remove extension if it exists and it's not a directory (though directories usually don't have extensions like files)
        // If it's a file, we want to strip the extension. e.g. test.txt -> test
        if (!firstFile.isDir) {
          baseName = baseName.replace(/\.[^/.]+$/, "");
        }

        const ext = encryptionMethod === 'SevenZip' ? '7z' : 'zip';
        defaultPath = `${baseName}_${encryptionMethod}.${ext}`;
      }

      const savePath = await save({
        filters,
        defaultPath,
      });

      if (savePath) {
        const result = await invoke("encrypt_files", {
          filePaths: droppedFiles.map(f => f.path),
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
      setDroppedFiles([]);
    }
  };

  const decryptFiles = async () => {
    if (droppedFiles.length === 0) {
      setErrorMessage(
        "Veuillez sélectionner ou glisser-déposer des fichiers d'abord."
      );
      return;
    }

    if (mode === 'encrypt' && !password) {
      setErrorMessage("Veuillez entrer ou générer un mot de passe.");
      return;
    }

    setErrorMessage(null);
    setIsEncrypting(true);
    setProgress(0);

    try {
      const outputDir = await open({
        directory: true,
        multiple: false,
      });

      if (outputDir) {
        for (const file of droppedFiles) {
          await invoke("decrypt_file", {
            filePath: file.path,
            outputDir,
            password,
          });
        }
        setSuccessMessage(`Fichiers déchiffrés vers ${outputDir}`);
        setZipOutputPath(outputDir as string);
      } else {
        toast.info("Déchiffrement annulé par l'utilisateur.", { duration: 2000 });
      }
    } catch (error) {
      console.error("Decryption failed:", error);
      setErrorMessage(`Échec du déchiffrement : ${error}`);
    } finally {
      setIsEncrypting(false);
      setProgress(0);
      setDroppedFiles([]);
    }
  };

  return (
    <div className="min-h-screen flex items-start justify-center p-4 font-sans">
      <Card className="w-full max-w-2xl mx-auto flex flex-col my-auto">
        <CardHeader>
          <CardTitle className="flex justify-center mb-6">
            <Logo className="w-full" />
          </CardTitle>
          <CardDescription className="text-lg text-center mb-4">
            Chiffrez et déchiffrez vos fichiers et dossiers.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col flex-grow">
          <Tabs value={mode} onValueChange={(v: string) => setMode(v as "encrypt" | "decrypt")} className="w-full">
            <TabsList className="grid w-full grid-cols-2 mb-6">
              <TabsTrigger value="encrypt">Chiffrer</TabsTrigger>
              <TabsTrigger value="decrypt">Déchiffrer</TabsTrigger>
            </TabsList>

            <div className="grid w-full items-center gap-6">
              <div
                className="flex flex-col items-center justify-center p-6 border-2 border-dashed rounded-lg min-h-[8rem] py-8"
                onClick={selectFiles}
              >
                {droppedFiles.length > 0 ? (
                  <div className="w-full">
                    <div className="flex flex-col sm:flex-row justify-between items-start sm:items-center mb-4 gap-2">
                      <p className="text-sm font-medium whitespace-nowrap">
                        Éléments sélectionné(s) :
                      </p>
                      <div className="flex flex-wrap gap-2">
                        <Button variant="outline" size="sm" onClick={selectFiles}>
                          + Fichiers
                        </Button>
                        {mode === 'encrypt' && (
                          <Button variant="outline" size="sm" onClick={selectFolder}>
                            + Dossier
                          </Button>
                        )}
                        <Button variant="ghost" size="sm" onClick={clearFiles}>
                          Tout effacer
                        </Button>
                      </div>
                    </div>
                    <ul className="list-none p-0 m-0 space-y-1 max-h-32 overflow-y-auto scroll-fade">
                      {droppedFiles.map((file, index) => (
                        <li
                          key={index}
                          className="flex items-center justify-between text-xs bg-secondary p-1.5 rounded-md"
                        >
                          <div className="flex items-center gap-2 truncate max-w-[300px]" title={file.path}>
                            {file.isDir ? <FolderIcon className="w-4 h-4 text-blue-500 flex-shrink-0" /> : <FileIcon className="w-4 h-4 text-gray-500 flex-shrink-0" />}
                            <span className="truncate">{file.path.split("/").pop()}</span>
                          </div>
                          <Button
                            variant="ghost"
                            size="icon"
                            onClick={(e) => {
                              e.stopPropagation();
                              removeFile(file.path);
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
                  <div className="flex flex-col items-center gap-4">
                    <p className="text-center text-muted-foreground">
                      {mode === 'encrypt' ? "Glissez-déposez des fichiers ou dossiers ici" : "Glissez-déposez des fichiers ici"}
                    </p>
                    <div className="relative flex items-center justify-center w-full">
                      <div className="absolute inset-0 flex items-center">
                        <span className="w-full border-t" />
                      </div>
                      <div className="relative flex justify-center text-xs uppercase">
                        <span className="bg-background px-2 text-muted-foreground">
                          Ou
                        </span>
                      </div>
                    </div>
                    <div className="flex gap-4">
                      <Button onClick={selectFiles}>Sélectionner Fichiers</Button>
                      {mode === 'encrypt' && (
                        <Button variant="secondary" onClick={selectFolder}>Sélectionner Dossier</Button>
                      )}
                    </div>
                  </div>
                )}
              </div>
              <div className="flex flex-col gap-4">
                <div className="flex items-center space-x-2">
                  <Input
                    id="password"
                    placeholder={mode === 'encrypt' ? "Entrez votre mot de passe" : "Mot de passe (laisser vide si non chiffré)"}
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
                {mode === 'encrypt' && (
                  <Button variant="outline" onClick={generatePassword}>
                    Générer un mot de passe
                  </Button>
                )}
              </div>
              {mode === 'encrypt' && (
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
              )}
            </div>
          </Tabs>
        </CardContent>
        <CardFooter className="flex flex-col space-y-4">
          {errorMessage && (
            <p className="text-destructive text-sm text-center w-full">
              {errorMessage}
            </p>
          )}
          <Button
            onClick={isEncrypting ? handleCancel : (mode === 'encrypt' ? encryptFiles : decryptFiles)}
            disabled={droppedFiles.length === 0 || (mode === 'encrypt' && !password)}
            className="w-full"
            variant={isEncrypting ? "destructive" : "default"}
          >
            {isEncrypting ? "Annuler" : (mode === 'encrypt' ? "Chiffrer" : "Déchiffrer")}
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
                {mode === 'encrypt' ? "Archive exportée vers :" : "Fichiers déchiffrés vers :"}
                <br />
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
              <Toaster />
            </Alert>
          )}
        </CardFooter>
      </Card>

    </div>
  );
}


export default App;
